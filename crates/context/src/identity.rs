use std::{collections::VecDeque, path::Path};

use crate::schema::{ByteRange, ChunkId, ChunkRecord};

/// A stable, cross-run identifier for a code chunk.
///
/// `StableId` survives byte-offset shifts and minor edits.
/// Named chunks are identified by `(path, kind, name, parent)`.
/// Unnamed chunks fall back to `(path, kind, content_hash, parent)`.
#[derive(
    Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct StableId(pub String);

impl StableId {
    /// Compute a stable identifier from chunk metadata and source bytes.
    #[must_use]
    pub fn compute(
        path: &Path,
        kind: &str,
        name: Option<&str>,
        parent: Option<&ChunkId>,
        source: &[u8],
        byte_range: &ByteRange,
    ) -> Self {
        let mut digest = StableDigest::new();
        digest.write_field(normalized_path(path).as_bytes());
        digest.write_field(kind.as_bytes());
        digest.write_optional_field(name.map(str::as_bytes));
        digest.write_optional_field(parent.map(|p| p.kind.as_bytes()));
        digest.write_optional_field(parent.and_then(|p| p.name.as_deref().map(str::as_bytes)));

        if name.is_some() {
            Self(format!("named:{:032x}", digest.finish()))
        } else {
            let content = &source[byte_range.start..byte_range.end.min(source.len())];
            digest.write_field(content);
            Self(format!("unnamed:{:032x}", digest.finish()))
        }
    }
}

/// Deterministic 128-bit FNV-1a digest for stable IDs.
///
/// This is intentionally explicit instead of using `DefaultHasher`, whose
/// output is not a public stability contract.
pub(crate) struct StableDigest(u128);

impl StableDigest {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

    pub(crate) const fn new() -> Self {
        Self(Self::OFFSET)
    }

    pub(crate) fn write_field(&mut self, bytes: &[u8]) {
        self.write_bytes(&(bytes.len() as u64).to_le_bytes());
        self.write_bytes(bytes);
    }

    pub(crate) fn write_optional_field(&mut self, bytes: Option<&[u8]>) {
        match bytes {
            Some(bytes) => {
                self.write_bytes(&[1]);
                self.write_field(bytes);
            }
            None => self.write_bytes(&[0]),
        }
    }

    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u128::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    pub(crate) const fn finish(self) -> u128 {
        self.0
    }
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Result of matching a chunk across two parse runs.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MatchResult {
    /// The chunk exists in both old and new with the same stable identity.
    Unchanged { old: ChunkRecord, new: ChunkRecord },
    /// The chunk only exists in the old snapshot.
    Removed { old: ChunkRecord },
    /// The chunk only exists in the new snapshot.
    Added { new: ChunkRecord },
}

/// Match chunks from an old snapshot against chunks from a new snapshot.
///
/// Returns one `MatchResult` per chunk across both sets.
#[must_use]
pub fn match_chunks(old: &[ChunkRecord], new: &[ChunkRecord]) -> Vec<MatchResult> {
    use std::collections::HashMap;

    let mut new_by_id: HashMap<_, VecDeque<_>> = HashMap::new();
    for chunk in new {
        new_by_id
            .entry(chunk.stable_id.clone())
            .or_default()
            .push_back(chunk.clone());
    }

    let mut results = Vec::new();

    // Find unchanged and removed in old traversal order.
    for old_chunk in old {
        if let Some(new_chunks) = new_by_id.get_mut(&old_chunk.stable_id)
            && let Some(new_chunk) = new_chunks.pop_front()
        {
            results.push(MatchResult::Unchanged {
                old: old_chunk.clone(),
                new: new_chunk,
            });
        } else {
            results.push(MatchResult::Removed {
                old: old_chunk.clone(),
            });
        }
    }

    // Find added in new traversal order.
    for new_chunk in new {
        if let Some(new_chunks) = new_by_id.get_mut(&new_chunk.stable_id)
            && let Some(remaining) = new_chunks.pop_front()
        {
            results.push(MatchResult::Added { new: remaining });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dummy_chunk(name: Option<&str>, stable_id: StableId) -> ChunkRecord {
        dummy_chunk_at(name, stable_id, 0)
    }

    fn dummy_chunk_at(name: Option<&str>, stable_id: StableId, anchor_byte: usize) -> ChunkRecord {
        ChunkRecord {
            id: crate::schema::ChunkId {
                path: PathBuf::from("test.rs"),
                kind: "function_item".to_string(),
                name: name.map(String::from),
                anchor_byte,
            },
            kind: "function_item".to_string(),
            name: name.map(String::from),
            byte_range: ByteRange {
                start: anchor_byte,
                end: anchor_byte + 10,
            },
            estimated_tokens: 2,
            confidence: crate::schema::Confidence::Exact,
            stable_id,
            depth: 0,
            parent: None,
            signature_hash: "sig_hash".to_string(),
            body_hash: "body_hash".to_string(),
        }
    }

    #[test]
    fn named_chunk_survives_anchor_change() {
        let source = b"fn foo() {}";
        let range = ByteRange { start: 0, end: 11 };

        let id1 = StableId::compute(
            Path::new("src/lib.rs"),
            "function_item",
            Some("foo"),
            None,
            source,
            &range,
        );
        let id2 = StableId::compute(
            Path::new("src/lib.rs"),
            "function_item",
            Some("foo"),
            None,
            source,
            &range,
        );

        assert_eq!(id1, id2);
    }

    #[test]
    fn named_chunk_changes_on_rename() {
        let source = b"fn foo() {}";
        let range = ByteRange { start: 0, end: 11 };

        let id1 = StableId::compute(
            Path::new("src/lib.rs"),
            "function_item",
            Some("foo"),
            None,
            source,
            &range,
        );
        let id2 = StableId::compute(
            Path::new("src/lib.rs"),
            "function_item",
            Some("bar"),
            None,
            source,
            &range,
        );

        assert_ne!(id1, id2);
    }

    #[test]
    fn unnamed_chunk_changes_on_content_edit() {
        let source1 = b"{ let x = 1; }";
        let source2 = b"{ let x = 2; }";
        let range = ByteRange { start: 0, end: 14 };

        let id1 = StableId::compute(
            Path::new("src/lib.rs"),
            "block",
            None,
            None,
            source1,
            &range,
        );
        let id2 = StableId::compute(
            Path::new("src/lib.rs"),
            "block",
            None,
            None,
            source2,
            &range,
        );

        assert_ne!(id1, id2);
    }

    #[test]
    fn unnamed_chunk_survives_position_change_with_same_content() {
        let source = b"fn foo() { let x = 1; }";
        let range1 = ByteRange { start: 11, end: 23 };
        let range2 = ByteRange { start: 11, end: 23 };

        let id1 = StableId::compute(
            Path::new("src/lib.rs"),
            "block",
            None,
            None,
            source,
            &range1,
        );
        let id2 = StableId::compute(
            Path::new("src/lib.rs"),
            "block",
            None,
            None,
            source,
            &range2,
        );

        assert_eq!(id1, id2);
    }

    #[test]
    fn match_chunks_finds_unchanged() {
        let old = vec![dummy_chunk(Some("foo"), StableId("named:a".to_string()))];
        let new = vec![dummy_chunk(Some("foo"), StableId("named:a".to_string()))];

        let results = match_chunks(&old, &new);
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], MatchResult::Unchanged { .. }));
    }

    #[test]
    fn match_chunks_finds_removed_and_added() {
        let old = vec![dummy_chunk(Some("foo"), StableId("named:a".to_string()))];
        let new = vec![dummy_chunk(Some("bar"), StableId("named:b".to_string()))];

        let results = match_chunks(&old, &new);
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .any(|r| matches!(r, MatchResult::Removed { .. }))
        );
        assert!(
            results
                .iter()
                .any(|r| matches!(r, MatchResult::Added { .. }))
        );
    }

    #[test]
    fn match_chunks_preserves_duplicate_stable_ids() {
        let old = vec![
            dummy_chunk_at(Some("foo"), StableId("named:a".to_string()), 0),
            dummy_chunk_at(Some("foo"), StableId("named:a".to_string()), 20),
        ];
        let new = vec![
            dummy_chunk_at(Some("foo"), StableId("named:a".to_string()), 0),
            dummy_chunk_at(Some("foo"), StableId("named:a".to_string()), 20),
        ];

        let results = match_chunks(&old, &new);

        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|r| matches!(r, MatchResult::Unchanged { .. }))
        );
    }

    #[test]
    fn match_chunks_reports_extra_duplicate_as_added() {
        let old = vec![dummy_chunk_at(
            Some("foo"),
            StableId("named:a".to_string()),
            0,
        )];
        let new = vec![
            dummy_chunk_at(Some("foo"), StableId("named:a".to_string()), 0),
            dummy_chunk_at(Some("foo"), StableId("named:a".to_string()), 20),
        ];

        let results = match_chunks(&old, &new);

        assert_eq!(results.len(), 2);
        assert!(matches!(results[0], MatchResult::Unchanged { .. }));
        assert!(matches!(results[1], MatchResult::Added { .. }));
    }

    #[test]
    fn collision_resistance_for_different_paths() {
        let source = b"fn foo() {}";
        let range = ByteRange { start: 0, end: 11 };

        let id1 = StableId::compute(
            Path::new("a.rs"),
            "function_item",
            Some("foo"),
            None,
            source,
            &range,
        );
        let id2 = StableId::compute(
            Path::new("b.rs"),
            "function_item",
            Some("foo"),
            None,
            source,
            &range,
        );

        assert_ne!(id1, id2);
    }
}

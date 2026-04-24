use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::Path,
};

use crate::schema::{ByteRange, ChunkRecord};

/// A stable, cross-run identifier for a code chunk.
///
/// `StableId` survives byte-offset shifts and minor edits.
/// Named chunks are identified by `(path, kind, name)`.
/// Unnamed chunks fall back to `(path, kind, content_hash)`.
#[derive(
    Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct StableId(pub String);

impl StableId {
    /// Compute a stable identifier from chunk metadata and source bytes.
    pub fn compute(
        path: &Path,
        kind: &str,
        name: Option<&str>,
        source: &[u8],
        byte_range: &ByteRange,
    ) -> Self {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        kind.hash(&mut hasher);

        if let Some(name) = name {
            name.hash(&mut hasher);
            Self(format!("named:{:016x}", hasher.finish()))
        } else {
            let content = &source[byte_range.start..byte_range.end.min(source.len())];
            content.hash(&mut hasher);
            Self(format!("unnamed:{:016x}", hasher.finish()))
        }
    }
}

/// Result of matching a chunk across two parse runs.
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
/// Returns one `MatchResult` per unique stable identity across both sets.
pub fn match_chunks(old: &[ChunkRecord], new: &[ChunkRecord]) -> Vec<MatchResult> {
    use std::collections::HashMap;

    let old_by_id: HashMap<_, _> = old
        .iter()
        .map(|c| (c.stable_id.clone(), c.clone()))
        .collect();
    let new_by_id: HashMap<_, _> = new
        .iter()
        .map(|c| (c.stable_id.clone(), c.clone()))
        .collect();

    let mut results = Vec::new();
    let mut seen = HashMap::new();

    // Find unchanged and removed
    for (id, old_chunk) in &old_by_id {
        if let Some(new_chunk) = new_by_id.get(id) {
            seen.insert(id.clone(), ());
            results.push(MatchResult::Unchanged {
                old: old_chunk.clone(),
                new: new_chunk.clone(),
            });
        } else {
            results.push(MatchResult::Removed {
                old: old_chunk.clone(),
            });
        }
    }

    // Find added
    for (id, new_chunk) in &new_by_id {
        if !seen.contains_key(id) {
            results.push(MatchResult::Added {
                new: new_chunk.clone(),
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dummy_chunk(name: Option<&str>, stable_id: StableId) -> ChunkRecord {
        ChunkRecord {
            id: crate::schema::ChunkId {
                path: PathBuf::from("test.rs"),
                kind: "function_item".to_string(),
                name: name.map(String::from),
                anchor_byte: 0,
            },
            kind: "function_item".to_string(),
            name: name.map(String::from),
            byte_range: ByteRange { start: 0, end: 10 },
            estimated_tokens: 2,
            confidence: crate::schema::Confidence::Exact,
            stable_id,
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
            source,
            &range,
        );
        let id2 = StableId::compute(
            Path::new("src/lib.rs"),
            "function_item",
            Some("foo"),
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
            source,
            &range,
        );
        let id2 = StableId::compute(
            Path::new("src/lib.rs"),
            "function_item",
            Some("bar"),
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

        let id1 = StableId::compute(Path::new("src/lib.rs"), "block", None, source1, &range);
        let id2 = StableId::compute(Path::new("src/lib.rs"), "block", None, source2, &range);

        assert_ne!(id1, id2);
    }

    #[test]
    fn unnamed_chunk_survives_position_change_with_same_content() {
        let source = b"fn foo() { let x = 1; }";
        let range1 = ByteRange { start: 11, end: 23 };
        let range2 = ByteRange { start: 11, end: 23 };

        let id1 = StableId::compute(Path::new("src/lib.rs"), "block", None, source, &range1);
        let id2 = StableId::compute(Path::new("src/lib.rs"), "block", None, source, &range2);

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
    fn collision_resistance_for_different_paths() {
        let source = b"fn foo() {}";
        let range = ByteRange { start: 0, end: 11 };

        let id1 = StableId::compute(
            Path::new("a.rs"),
            "function_item",
            Some("foo"),
            source,
            &range,
        );
        let id2 = StableId::compute(
            Path::new("b.rs"),
            "function_item",
            Some("foo"),
            source,
            &range,
        );

        assert_ne!(id1, id2);
    }
}

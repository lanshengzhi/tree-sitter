//! Budgeted context bundling.
//!
//! Selects chunks to fit within a token budget, prioritizing by
//! structural importance and optional relevance hints.

use crate::schema::{ChunkRecord, Diagnostic};

/// Reason a chunk was omitted from the bundle.
#[derive(
    Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OmissionReason {
    /// The chunk would exceed the remaining token budget.
    OverBudget,
    /// The chunk was deprioritized in favor of more important chunks.
    LowPriority,
}

/// A chunk that was considered but not included in the bundle.
#[derive(
    Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct OmittedChunk {
    pub chunk: ChunkRecord,
    pub reason: OmissionReason,
}

/// Result of a budgeted bundling pass.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct BundleOutput {
    pub included: Vec<ChunkRecord>,
    pub omitted: Vec<OmittedChunk>,
    pub total_included_tokens: usize,
    pub total_omitted_tokens: usize,
    pub budget: usize,
    pub diagnostics: Vec<Diagnostic>,
}

/// Options for bundling.
#[derive(Clone, Debug)]
pub struct BundleOptions {
    pub max_tokens: usize,
    /// Maximum number of chunks to include in the bundle.
    pub max_chunks: usize,
}

impl Default for BundleOptions {
    fn default() -> Self {
        Self {
            max_tokens: 2_000,
            max_chunks: 100,
        }
    }
}

/// Build a budgeted bundle from a list of chunks.
///
/// Uses a greedy algorithm:
/// 1. Sort chunks by estimated token count (smaller first) to maximize coverage.
/// 2. Add chunks until the budget is exhausted.
/// 3. Return included chunks and omitted chunks with reasons.
///
/// In the future this can be enhanced with:
/// - Symbol-aware priority (boost chunks containing a target symbol)
/// - Invalidation-aware priority (boost recently changed chunks)
/// - Depth-based priority (boost top-level declarations)
#[must_use]
pub fn bundle_chunks(chunks: Vec<ChunkRecord>, opts: &BundleOptions) -> BundleOutput {
    let mut included = Vec::new();
    let mut omitted = Vec::new();
    let mut total_included = 0usize;
    let mut total_omitted = 0usize;
    let mut diagnostics = Vec::new();

    // Sort by depth ascending (top-level first), then by token count ascending
    // (smaller first) to maximize coverage within each depth level.
    let mut sorted = chunks;
    sorted.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.estimated_tokens.cmp(&b.estimated_tokens))
    });

    for chunk in sorted {
        let tokens = chunk.estimated_tokens;

        if tokens > opts.max_tokens {
            // Chunk is larger than the entire budget; omit it.
            total_omitted += tokens;
            omitted.push(OmittedChunk {
                chunk,
                reason: OmissionReason::OverBudget,
            });
            continue;
        }

        if included.len() >= opts.max_chunks {
            total_omitted += tokens;
            omitted.push(OmittedChunk {
                chunk,
                reason: OmissionReason::LowPriority,
            });
        } else if total_included + tokens <= opts.max_tokens {
            total_included += tokens;
            included.push(chunk);
        } else {
            total_omitted += tokens;
            omitted.push(OmittedChunk {
                chunk,
                reason: OmissionReason::OverBudget,
            });
        }
    }

    if !omitted.is_empty() {
        diagnostics.push(Diagnostic::info(format!(
            "{} chunk(s) omitted due to budget ({}/{} tokens used)",
            omitted.len(),
            total_included,
            opts.max_tokens,
        )));
    }

    BundleOutput {
        included,
        omitted,
        total_included_tokens: total_included,
        total_omitted_tokens: total_omitted,
        budget: opts.max_tokens,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::StableId;
    use crate::schema::{ByteRange, ChunkId, Confidence};
    use std::path::PathBuf;

    fn dummy_chunk(name: &str, tokens: usize) -> ChunkRecord {
        ChunkRecord {
            id: ChunkId {
                path: PathBuf::from("test.rs"),
                kind: "function_item".to_string(),
                name: Some(name.to_string()),
                anchor_byte: 0,
            },
            stable_id: StableId(format!("named:{name}")),
            kind: "function_item".to_string(),
            name: Some(name.to_string()),
            byte_range: ByteRange {
                start: 0,
                end: tokens * 4,
            },
            estimated_tokens: tokens,
            confidence: Confidence::Exact,
            depth: 0,
            parent: None,
            signature_hash: "sig_hash".to_string(),
            body_hash: "body_hash".to_string(),
        }
    }

    #[test]
    fn fits_all_chunks_within_budget() {
        let chunks = vec![
            dummy_chunk("foo", 100),
            dummy_chunk("bar", 200),
            dummy_chunk("baz", 300),
        ];
        let opts = BundleOptions {
            max_tokens: 1_000,
            max_chunks: 100,
        };
        let bundle = bundle_chunks(chunks, &opts);

        assert_eq!(bundle.included.len(), 3);
        assert!(bundle.omitted.is_empty());
        assert_eq!(bundle.total_included_tokens, 600);
    }

    #[test]
    fn omits_chunks_over_budget() {
        let chunks = vec![
            dummy_chunk("foo", 100),
            dummy_chunk("bar", 200),
            dummy_chunk("baz", 300),
        ];
        let opts = BundleOptions {
            max_tokens: 400,
            max_chunks: 100,
        };
        let bundle = bundle_chunks(chunks, &opts);

        assert_eq!(bundle.included.len(), 2); // 100 + 200
        assert_eq!(bundle.omitted.len(), 1); // 300
        assert_eq!(bundle.total_included_tokens, 300);
        assert_eq!(bundle.omitted[0].reason, OmissionReason::OverBudget);
    }

    #[test]
    fn omits_single_chunk_larger_than_budget() {
        let chunks = vec![dummy_chunk("huge", 5_000)];
        let opts = BundleOptions {
            max_tokens: 2_000,
            max_chunks: 100,
        };
        let bundle = bundle_chunks(chunks, &opts);

        assert!(bundle.included.is_empty());
        assert_eq!(bundle.omitted.len(), 1);
        assert_eq!(bundle.omitted[0].reason, OmissionReason::OverBudget);
    }

    #[test]
    fn empty_input_returns_empty_bundle() {
        let bundle = bundle_chunks(Vec::new(), &BundleOptions::default());
        assert!(bundle.included.is_empty());
        assert!(bundle.omitted.is_empty());
        assert_eq!(bundle.total_included_tokens, 0);
    }

    #[test]
    fn bundle_output_snapshot_stability() {
        let chunks = vec![dummy_chunk("foo", 100), dummy_chunk("bar", 500)];
        let opts = BundleOptions {
            max_tokens: 250,
            max_chunks: 100,
        };
        let bundle = bundle_chunks(chunks, &opts);

        let json = serde_json::to_string_pretty(&bundle).unwrap();

        assert_eq!(
            json,
            r#"{
  "included": [
    {
      "id": {
        "path": "test.rs",
        "kind": "function_item",
        "name": "foo",
        "anchor_byte": 0
      },
      "stable_id": "named:foo",
      "kind": "function_item",
      "name": "foo",
      "byte_range": {
        "start": 0,
        "end": 400
      },
      "estimated_tokens": 100,
      "confidence": "exact",
      "signature_hash": "sig_hash",
      "body_hash": "body_hash"
    }
  ],
  "omitted": [
    {
      "chunk": {
        "id": {
          "path": "test.rs",
          "kind": "function_item",
          "name": "bar",
          "anchor_byte": 0
        },
        "stable_id": "named:bar",
        "kind": "function_item",
        "name": "bar",
        "byte_range": {
          "start": 0,
          "end": 2000
        },
        "estimated_tokens": 500,
        "confidence": "exact",
        "signature_hash": "sig_hash",
        "body_hash": "body_hash"
      },
      "reason": "over_budget"
    }
  ],
  "total_included_tokens": 100,
  "total_omitted_tokens": 500,
  "budget": 250,
  "diagnostics": [
    {
      "level": "info",
      "code": "general_info",
      "message": "1 chunk(s) omitted due to budget (100/250 tokens used)"
    }
  ]
}"#
        );

        let deserialized: BundleOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.included.len(), 1);
        assert_eq!(deserialized.omitted.len(), 1);
    }
}

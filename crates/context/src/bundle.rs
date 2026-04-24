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
}

impl Default for BundleOptions {
    fn default() -> Self {
        Self { max_tokens: 2_000 }
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
pub fn bundle_chunks(chunks: Vec<ChunkRecord>, opts: &BundleOptions) -> BundleOutput {
    let mut included = Vec::new();
    let mut omitted = Vec::new();
    let mut total_included = 0usize;
    let mut total_omitted = 0usize;
    let mut diagnostics = Vec::new();

    // Sort by token count ascending (smaller first) to fit more chunks.
    let mut sorted = chunks;
    sorted.sort_by_key(|c| c.estimated_tokens);

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

        if total_included + tokens <= opts.max_tokens {
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
            stable_id: StableId(format!("named:{}", name)),
            kind: "function_item".to_string(),
            name: Some(name.to_string()),
            byte_range: ByteRange {
                start: 0,
                end: tokens * 4,
            },
            estimated_tokens: tokens,
            confidence: Confidence::Exact,
        }
    }

    #[test]
    fn fits_all_chunks_within_budget() {
        let chunks = vec![
            dummy_chunk("foo", 100),
            dummy_chunk("bar", 200),
            dummy_chunk("baz", 300),
        ];
        let opts = BundleOptions { max_tokens: 1_000 };
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
        let opts = BundleOptions { max_tokens: 400 };
        let bundle = bundle_chunks(chunks, &opts);

        assert_eq!(bundle.included.len(), 2); // 100 + 200
        assert_eq!(bundle.omitted.len(), 1); // 300
        assert_eq!(bundle.total_included_tokens, 300);
        assert_eq!(bundle.omitted[0].reason, OmissionReason::OverBudget);
    }

    #[test]
    fn omits_single_chunk_larger_than_budget() {
        let chunks = vec![dummy_chunk("huge", 5_000)];
        let opts = BundleOptions { max_tokens: 2_000 };
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
}

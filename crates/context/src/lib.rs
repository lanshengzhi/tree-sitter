//! `tree-sitter-context` — extract token-efficient code context from
//! Tree-sitter syntax trees with stable identity, diagnostics, and
//! incremental invalidation.

pub mod chunk;
pub mod identity;
pub mod invalidation;
pub mod schema;

pub use chunk::{ChunkOptions, chunks_for_tree};
pub use identity::{StableId, match_chunks};
pub use invalidation::{invalidate_edits, invalidate_snapshot};
pub use schema::{
    ByteRange, ChunkId, ChunkRecord, Confidence, ContextOutput, Diagnostic, DiagnosticLevel,
    OutputMeta,
};

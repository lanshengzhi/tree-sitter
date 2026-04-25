//! `tree-sitter-context` — extract token-efficient code context from
//! Tree-sitter syntax trees with stable identity, diagnostics, and
//! incremental invalidation.

pub mod bundle;
pub mod chunk;
pub mod identity;
pub mod invalidation;
pub mod schema;
pub mod symbols;

pub use bundle::{BundleOptions, BundleOutput, OmissionReason, bundle_chunks};
pub use chunk::{ChunkOptions, ChunkOutput, chunks_for_tree};
pub use identity::{StableId, match_chunks};
pub use invalidation::{invalidate_edits, invalidate_snapshot};
pub use schema::{
    ByteRange, ChunkId, ChunkRecord, Confidence, ContextOutput, Diagnostic, DiagnosticLevel,
    InvalidationOutput, OutputMeta, SymbolRecord,
};

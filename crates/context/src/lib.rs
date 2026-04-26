//! `tree-sitter-context` — extract token-efficient code context from
//! Tree-sitter syntax trees with stable identity, diagnostics, and
//! incremental invalidation.

pub mod bundle;
pub mod chunk;
pub mod graph;
pub mod identity;
pub mod invalidation;
pub mod protocol;
pub mod schema;
pub mod sexpr;
pub mod symbols;

pub use bundle::{BundleOptions, BundleOutput, OmissionReason, bundle_chunks};
pub use chunk::{ChunkOptions, ChunkOutput, chunks_for_tree};
pub use graph::{
    canonicalize_snapshot, extract_graph_file, EdgeStatus, GraphError, GraphFile, GraphMeta,
    GraphNode, GraphNodeHandle, GraphSnapshot, GraphSnapshotId, GraphSymbol, GraphStore,
    GRAPH_SCHEMA_VERSION,
};
pub use graph::diff;
pub use identity::{StableId, match_chunks};
pub use invalidation::{invalidate_edits, invalidate_snapshot};
pub use schema::{
    ByteRange, ChunkId, ChunkRecord, Confidence, ContextOutput, Diagnostic, DiagnosticCode,
    DiagnosticLevel, InvalidationOutput, InvalidationReason, InvalidationRecord,
    InvalidationStatus, MatchStrategy, OutputMeta, SymbolRecord,
};

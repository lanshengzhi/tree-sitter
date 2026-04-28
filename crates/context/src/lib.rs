//! `tree-sitter-context` — extract token-efficient code context from
//! Tree-sitter syntax trees with stable identity, diagnostics, and
//! incremental invalidation.

pub mod bundle;
pub mod chunk;
pub mod compact;
pub mod graph;
pub mod identity;
pub mod invalidation;
pub mod orientation;
pub mod pagerank;
pub mod protocol;
pub mod schema;
pub mod sexpr;
pub mod snapshot_cache;
pub mod symbols;

pub use bundle::{BundleOptions, BundleOutput, OmissionReason, bundle_chunks};
pub use chunk::{ChunkOptions, ChunkOutput, chunks_for_tree};
pub use compact::{CompactError, CompactOptions, compact_files};
pub use orientation::{build_orientation, OrientationBlock, OrientationField, OrientationStats};
pub use pagerank::compute_god_nodes;
pub use graph::{
    canonicalize_snapshot, extract_graph_file, EdgeStatus, GraphError, GraphFile, GraphMeta,
    GraphNode, GraphNodeHandle, GraphSnapshot, GraphSnapshotId, GraphSymbol, GraphStore,
    GRAPH_SCHEMA_VERSION, GodNode, PostprocessStatus, POSTPROCESS_SCHEMA_VERSION,
    read_postprocess_artifact, write_postprocess_artifact,
};
pub use graph::diff;
pub use identity::{StableId, match_chunks};
pub use invalidation::{invalidate_edits, invalidate_snapshot};
pub use snapshot_cache::{CachedSnapshot, SnapshotCache};
pub use schema::{
    ByteRange, ChunkId, ChunkRecord, CompactChunkRecord, CompactFileResult, CompactOmittedRecord,
    CompactOutput, Confidence, ContextOutput, Diagnostic, DiagnosticCode, DiagnosticLevel,
    InvalidationOutput, InvalidationReason, InvalidationRecord, InvalidationStatus, MatchStrategy,
    OutputMeta, SymbolRecord,
};

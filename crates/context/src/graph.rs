//! `tree-sitter-context` graph substrate.
//!
//! Provides typed graph records, deterministic snapshot identity,
//! canonical serialization, and cross-file reference data structures.

pub mod diff;
pub mod extract;
pub mod postprocess;
pub mod snapshot;
pub mod store;
pub mod xref;

pub use extract::extract_graph_file;
pub use postprocess::{
    GodNode, PostprocessArtifact, PostprocessStatus, POSTPROCESS_SCHEMA_VERSION,
    read_postprocess_artifact, write_postprocess_artifact,
};
pub use snapshot::{
    canonicalize_snapshot, EdgeStatus, GraphError, GraphFile, GraphMeta, GraphNode,
    GraphNodeHandle, GraphSnapshot, GraphSnapshotId, GraphSymbol, GRAPH_SCHEMA_VERSION,
};
pub use store::{GraphStore, HEAD_FILE_NAME, STORE_DIR_NAME};

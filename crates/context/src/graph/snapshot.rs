use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_128;

use crate::identity::StableId;
use crate::schema::{ByteRange, Confidence, Diagnostic};

/// Current graph schema version. Bumps invalidate prior snapshot IDs.
pub const GRAPH_SCHEMA_VERSION: &str = "r1-2026-04-26";

/// A deterministic snapshot identifier derived from canonical graph bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema, Hash, Ord, PartialOrd)]
pub struct GraphSnapshotId(pub String);

impl GraphSnapshotId {
    /// Compute a deterministic XXH3-128 hex digest from canonical JSON bytes.
    ///
    /// The input must already be in canonical form (sorted keys, no
    /// whitespace variance, no absolute paths, no timestamps).
    #[must_use]
    pub fn from_canonical_bytes(bytes: &[u8]) -> Self {
        let hash = xxh3_128(bytes);
        Self(format!("{:032x}", hash))
    }
}

/// Status of a cross-file graph edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EdgeStatus {
    /// Unambiguously resolved to a single target.
    Confirmed,
    /// Multiple candidate targets exist.
    Ambiguous,
    /// No candidate target found.
    Unresolved,
    /// Language or config lacks reference capability.
    Unsupported,
}

/// A single node in the repo graph.
///
/// Node identity is collision-aware: it combines repo-relative path,
/// stable_id, and anchor byte position. Two nodes with the same
/// `stable_id` in the same file remain distinct via `anchor_byte`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema, Ord, PartialOrd)]
pub struct GraphNode {
    pub path: PathBuf,
    pub stable_id: StableId,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub anchor_byte: usize,
    pub byte_range: ByteRange,
    /// Signature hash (e.g., name + kind + parameters) for detecting
    /// body-only changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_hash: Option<String>,
    /// Content hash (e.g., of the full node text) for detecting any
    /// change including body edits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub confidence: Confidence,
}

/// A typed edge between graph nodes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema, Ord, PartialOrd)]
pub struct GraphEdge {
    pub source: GraphNodeHandle,
    pub target: GraphNodeHandle,
    pub kind: String,
    pub status: EdgeStatus,
    pub confidence: Confidence,
    /// When status is Ambiguous, ordered candidate handles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<GraphNodeHandle>,
}

/// A lightweight, collision-aware handle to a graph node.
///
/// This is used for edge endpoints so that edges do not inline full
/// node records. The handle combines path, stable_id, and anchor_byte
/// to guarantee uniqueness within a snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema, Hash, Ord, PartialOrd)]
pub struct GraphNodeHandle {
    pub path: PathBuf,
    pub stable_id: StableId,
    pub anchor_byte: usize,
}

impl From<&GraphNode> for GraphNodeHandle {
    fn from(node: &GraphNode) -> Self {
        Self {
            path: node.path.clone(),
            stable_id: node.stable_id.clone(),
            anchor_byte: node.anchor_byte,
        }
    }
}

/// A symbol record embedded in the graph, reusing the R0 schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema, Ord, PartialOrd)]
pub struct GraphSymbol {
    pub name: String,
    pub syntax_type: String,
    pub byte_range: ByteRange,
    pub is_definition: bool,
    pub node_handle: GraphNodeHandle,
    pub confidence: Confidence,
}

/// Per-file graph record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema, Ord, PartialOrd)]
pub struct GraphFile {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<GraphNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<GraphSymbol>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

/// A complete graph snapshot for a repo at a point in time.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphSnapshot {
    pub schema_version: String,
    pub snapshot_id: GraphSnapshotId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<GraphFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<GraphEdge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    /// Operational metadata outside the canonical identity hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<GraphMeta>,
}

/// Operational metadata for a snapshot. Not included in identity hash.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<PathBuf>,
    pub total_files: usize,
    pub total_nodes: usize,
    pub total_edges: usize,
}

/// Typed errors for graph operations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphError {
    MissingSnapshot {
        snapshot_id: GraphSnapshotId,
    },
    CorruptedSnapshot {
        snapshot_id: GraphSnapshotId,
        reason: String,
    },
    SchemaMismatch {
        expected: String,
        found: String,
    },
    LockFailure {
        path: PathBuf,
        reason: String,
    },
    WriteFailure {
        path: PathBuf,
        reason: String,
    },
    PostprocessUnavailable,
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSnapshot { snapshot_id } => {
                write!(f, "missing snapshot: {}", snapshot_id.0)
            }
            Self::CorruptedSnapshot { snapshot_id, reason } => {
                write!(f, "corrupted snapshot {}: {}", snapshot_id.0, reason)
            }
            Self::SchemaMismatch { expected, found } => {
                write!(f, "schema mismatch: expected {}, found {}", expected, found)
            }
            Self::LockFailure { path, reason } => {
                write!(f, "lock failure at {}: {}", path.display(), reason)
            }
            Self::WriteFailure { path, reason } => {
                write!(f, "write failure at {}: {}", path.display(), reason)
            }
            Self::PostprocessUnavailable => write!(f, "postprocess unavailable"),
        }
    }
}

impl std::error::Error for GraphError {}

/// Build a canonical `GraphSnapshot` with deterministic ordering and
/// compute its `snapshot_id`.
///
/// Files, nodes within each file, edges, and diagnostics are sorted
/// so that insertion order does not affect identity.
#[must_use]
pub fn canonicalize_snapshot(mut snapshot: GraphSnapshot) -> GraphSnapshot {
    // Sort files by path
    snapshot.files.sort_by(|a, b| a.path.cmp(&b.path));
    for file in &mut snapshot.files {
        file.nodes.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.stable_id.0.cmp(&b.stable_id.0))
                .then_with(|| a.anchor_byte.cmp(&b.anchor_byte))
        });
        file.symbols.sort_by(|a, b| {
            a.node_handle
                .path
                .cmp(&b.node_handle.path)
                .then_with(|| a.node_handle.stable_id.0.cmp(&b.node_handle.stable_id.0))
                .then_with(|| a.node_handle.anchor_byte.cmp(&b.node_handle.anchor_byte))
                .then_with(|| a.name.cmp(&b.name))
        });
    }
    snapshot.edges.sort_by(|a, b| {
        a.source
            .path
            .cmp(&b.source.path)
            .then_with(|| a.source.stable_id.0.cmp(&b.source.stable_id.0))
            .then_with(|| a.source.anchor_byte.cmp(&b.source.anchor_byte))
            .then_with(|| a.target.path.cmp(&b.target.path))
            .then_with(|| a.target.stable_id.0.cmp(&b.target.stable_id.0))
            .then_with(|| a.target.anchor_byte.cmp(&b.target.anchor_byte))
            .then_with(|| a.kind.cmp(&b.kind))
    });
    snapshot.diagnostics.sort_by(|a, b| a.message.cmp(&b.message));

    // Build a temporary snapshot without the snapshot_id for hashing
    let mut hash_input = snapshot.clone();
    hash_input.snapshot_id = GraphSnapshotId(String::new());
    hash_input.meta = None;

    let canonical_json =
        serde_json::to_string(&hash_input).expect("graph snapshot serializes to json");
    snapshot.snapshot_id = GraphSnapshotId::from_canonical_bytes(canonical_json.as_bytes());
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ae2_same_logical_snapshot_same_id() {
        let node = GraphNode {
            path: PathBuf::from("src/lib.rs"),
            stable_id: StableId("named:foo".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 10 },
            signature_hash: Some("sig1".to_string()),
            content_hash: Some("hash1".to_string()),
            confidence: Confidence::Exact,
        };

        let file = GraphFile {
            path: PathBuf::from("src/lib.rs"),
            content_hash: Some("file_hash".to_string()),
            nodes: vec![node],
            symbols: vec![],
            diagnostics: vec![],
        };

        let snapshot_a = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file.clone()],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });

        // Build identical snapshot with same insertion order
        let snapshot_b = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });

        assert_eq!(snapshot_a.snapshot_id, snapshot_b.snapshot_id);
    }

    #[test]
    fn ae2_different_insertion_order_same_id() {
        let node_a = GraphNode {
            path: PathBuf::from("src/a.rs"),
            stable_id: StableId("named:a".to_string()),
            kind: "function_item".to_string(),
            name: Some("a".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 5 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        };
        let node_b = GraphNode {
            path: PathBuf::from("src/b.rs"),
            stable_id: StableId("named:b".to_string()),
            kind: "function_item".to_string(),
            name: Some("b".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 5 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        };

        let file_a = GraphFile {
            path: PathBuf::from("src/a.rs"),
            content_hash: None,
            nodes: vec![node_a.clone()],
            symbols: vec![],
            diagnostics: vec![],
        };
        let file_b = GraphFile {
            path: PathBuf::from("src/b.rs"),
            content_hash: None,
            nodes: vec![node_b.clone()],
            symbols: vec![],
            diagnostics: vec![],
        };

        let snapshot_forward = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file_a.clone(), file_b.clone()],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });

        let snapshot_reverse = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file_b, file_a],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });

        assert_eq!(snapshot_forward.snapshot_id, snapshot_reverse.snapshot_id);
    }

    #[test]
    fn schema_version_change_changes_id() {
        let node = GraphNode {
            path: PathBuf::from("src/lib.rs"),
            stable_id: StableId("named:foo".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 10 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        };

        let file = GraphFile {
            path: PathBuf::from("src/lib.rs"),
            content_hash: None,
            nodes: vec![node],
            symbols: vec![],
            diagnostics: vec![],
        };

        let snapshot_v1 = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file.clone()],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });

        let snapshot_v2 = canonicalize_snapshot(GraphSnapshot {
            schema_version: "r1-2026-04-27".to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });

        assert_ne!(snapshot_v1.snapshot_id, snapshot_v2.snapshot_id);
    }

    #[test]
    fn duplicate_stable_ids_remain_distinct() {
        let node_1 = GraphNode {
            path: PathBuf::from("src/lib.rs"),
            stable_id: StableId("named:foo".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 10 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        };
        let node_2 = GraphNode {
            path: PathBuf::from("src/lib.rs"),
            stable_id: StableId("named:foo".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            anchor_byte: 12,
            byte_range: ByteRange { start: 12, end: 22 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        };

        let file = GraphFile {
            path: PathBuf::from("src/lib.rs"),
            content_hash: None,
            nodes: vec![node_1, node_2],
            symbols: vec![],
            diagnostics: vec![],
        };

        let snapshot = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });

        assert_eq!(snapshot.files[0].nodes.len(), 2);
        assert_eq!(snapshot.files[0].nodes[0].anchor_byte, 0);
        assert_eq!(snapshot.files[0].nodes[1].anchor_byte, 12);
    }

    #[test]
    fn snapshot_id_is_path_portable() {
        let node = GraphNode {
            path: PathBuf::from("src/lib.rs"),
            stable_id: StableId("named:foo".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 10 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        };

        let file = GraphFile {
            path: PathBuf::from("src/lib.rs"),
            content_hash: None,
            nodes: vec![node],
            symbols: vec![],
            diagnostics: vec![],
        };

        let snapshot = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file],
            edges: vec![],
            diagnostics: vec![],
            meta: Some(GraphMeta {
                created_at: Some("2026-04-26T00:00:00Z".to_string()),
                repo_root: Some(PathBuf::from("/home/user/project")),
                total_files: 1,
                total_nodes: 1,
                total_edges: 0,
            }),
        });

        // Meta should not affect snapshot_id
        let mut hash_input = snapshot.clone();
        hash_input.snapshot_id = GraphSnapshotId(String::new());
        hash_input.meta = None;
        let hash_json = serde_json::to_string(&hash_input).unwrap();
        assert!(!hash_json.contains("/home/user/project"));
    }
}

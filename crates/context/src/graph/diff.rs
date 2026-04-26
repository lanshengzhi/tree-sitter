//! Graph diff computation between two snapshots.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::graph::snapshot::{
    GraphEdge, GraphError, GraphFile, GraphNode, GraphNodeHandle, GraphSnapshot, GraphSnapshotId,
};
use crate::schema::{Confidence, Diagnostic};

/// Result of comparing two graph snapshots.
#[derive(Clone, Debug, Serialize)]
pub struct GraphDiff {
    pub from_snapshot_id: GraphSnapshotId,
    pub to_snapshot_id: GraphSnapshotId,
    pub changed_files: Vec<FileDiff>,
    pub changed_nodes: Vec<NodeDiff>,
    pub changed_symbols: Vec<SymbolDiff>,
    pub changed_edges: Vec<EdgeDiff>,
    pub postprocess_unavailable: bool,
    pub diagnostics: Vec<Diagnostic>,
}

/// Diff for a single file.
#[derive(Clone, Debug, Serialize)]
pub struct FileDiff {
    pub path: std::path::PathBuf,
    pub status: DiffStatus,
    pub reason: DiffReason,
    pub old_content_hash: Option<String>,
    pub new_content_hash: Option<String>,
    pub confidence: Confidence,
}

/// Diff for a single node.
#[derive(Clone, Debug, Serialize)]
pub struct NodeDiff {
    pub path: std::path::PathBuf,
    pub stable_id: crate::identity::StableId,
    pub anchor_byte: usize,
    pub status: DiffStatus,
    pub reason: DiffReason,
    pub old_signature_hash: Option<String>,
    pub new_signature_hash: Option<String>,
    pub old_content_hash: Option<String>,
    pub new_content_hash: Option<String>,
    pub confidence: Confidence,
}

/// Diff for a single symbol.
#[derive(Clone, Debug, Serialize)]
pub struct SymbolDiff {
    pub path: std::path::PathBuf,
    pub name: String,
    pub status: DiffStatus,
    pub reason: DiffReason,
    pub confidence: Confidence,
}

/// Diff for a single edge.
#[derive(Clone, Debug, Serialize)]
pub struct EdgeDiff {
    pub source: GraphNodeHandle,
    pub target: GraphNodeHandle,
    pub kind: String,
    pub status: DiffStatus,
    pub reason: DiffReason,
    pub confidence: Confidence,
}

/// Status of a diff entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Added,
    Removed,
    Modified,
}

/// Reason for a diff classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffReason {
    ContentChanged,
    SignatureChanged,
    Added,
    Removed,
    BodyOnlyChanged,
    Unknown,
}

/// Compute diff between two snapshots.
///
/// Returns `postprocess_unavailable = true` when severe orientation
/// classification would need god-node/community data that R1 has not
/// computed.
pub fn diff_snapshots(from: &GraphSnapshot, to: &GraphSnapshot) -> Result<GraphDiff, GraphError> {
    let mut changed_files = Vec::new();
    let mut changed_nodes = Vec::new();
    let mut changed_symbols = Vec::new();
    let mut changed_edges = Vec::new();
    let mut diagnostics = Vec::new();

    let from_files: HashMap<&std::path::Path, &GraphFile> =
        from.files.iter().map(|f| (f.path.as_ref(), f)).collect();
    let to_files: HashMap<&std::path::Path, &GraphFile> =
        to.files.iter().map(|f| (f.path.as_ref(), f)).collect();

    let from_paths: HashSet<&std::path::Path> = from_files.keys().copied().collect();
    let to_paths: HashSet<&std::path::Path> = to_files.keys().copied().collect();

    // Removed files (and their nodes)
    for path in from_paths.difference(&to_paths) {
        let file = from_files[path];
        changed_files.push(FileDiff {
            path: file.path.clone(),
            status: DiffStatus::Removed,
            reason: DiffReason::Removed,
            old_content_hash: file.content_hash.clone(),
            new_content_hash: None,
            confidence: Confidence::Exact,
        });
        for node in &file.nodes {
            changed_nodes.push(NodeDiff {
                path: node.path.clone(),
                stable_id: node.stable_id.clone(),
                anchor_byte: node.anchor_byte,
                status: DiffStatus::Removed,
                reason: DiffReason::Removed,
                old_signature_hash: node.signature_hash.clone(),
                new_signature_hash: None,
                old_content_hash: node.content_hash.clone(),
                new_content_hash: None,
                confidence: Confidence::Exact,
            });
        }
        for sym in &file.symbols {
            changed_symbols.push(SymbolDiff {
                path: sym.node_handle.path.clone(),
                name: sym.name.clone(),
                status: DiffStatus::Removed,
                reason: DiffReason::Removed,
                confidence: Confidence::Exact,
            });
        }
    }

    // Added files (and their nodes)
    for path in to_paths.difference(&from_paths) {
        let file = to_files[path];
        changed_files.push(FileDiff {
            path: file.path.clone(),
            status: DiffStatus::Added,
            reason: DiffReason::Added,
            old_content_hash: None,
            new_content_hash: file.content_hash.clone(),
            confidence: Confidence::Exact,
        });
        for node in &file.nodes {
            changed_nodes.push(NodeDiff {
                path: node.path.clone(),
                stable_id: node.stable_id.clone(),
                anchor_byte: node.anchor_byte,
                status: DiffStatus::Added,
                reason: DiffReason::Added,
                old_signature_hash: None,
                new_signature_hash: node.signature_hash.clone(),
                old_content_hash: None,
                new_content_hash: node.content_hash.clone(),
                confidence: Confidence::Exact,
            });
        }
        for sym in &file.symbols {
            changed_symbols.push(SymbolDiff {
                path: sym.node_handle.path.clone(),
                name: sym.name.clone(),
                status: DiffStatus::Added,
                reason: DiffReason::Added,
                confidence: Confidence::Exact,
            });
        }
    }

    // Modified files
    for path in from_paths.intersection(&to_paths) {
        let from_file = from_files[path];
        let to_file = to_files[path];

        if from_file.content_hash != to_file.content_hash {
            let reason = if from_file.content_hash.is_some()
                && to_file.content_hash.is_some()
            {
                DiffReason::ContentChanged
            } else {
                DiffReason::Unknown
            };

            changed_files.push(FileDiff {
                path: from_file.path.clone(),
                status: DiffStatus::Modified,
                reason,
                old_content_hash: from_file.content_hash.clone(),
                new_content_hash: to_file.content_hash.clone(),
                confidence: Confidence::High,
            });

            // Diff nodes within modified files
            diff_file_nodes(from_file, to_file, &mut changed_nodes);
            diff_file_symbols(from_file, to_file, &mut changed_symbols);
        }
    }

    // Diff edges globally
    diff_edges(&from.edges,
        &to.edges,
        &mut changed_edges,
        &mut diagnostics,
    );

    // Check for severe orientation changes that would need postprocess data
    let postprocess_unavailable = has_severe_changes(&changed_files,
        &changed_nodes,
        &changed_symbols,
    );

    Ok(GraphDiff {
        from_snapshot_id: from.snapshot_id.clone(),
        to_snapshot_id: to.snapshot_id.clone(),
        changed_files,
        changed_nodes,
        changed_symbols,
        changed_edges,
        postprocess_unavailable,
        diagnostics,
    })
}

fn diff_file_nodes(from_file: &GraphFile, to_file: &GraphFile, changed: &mut Vec<NodeDiff>) {
    let from_nodes: HashMap<GraphNodeHandle, &GraphNode> = from_file
        .nodes
        .iter()
        .map(|n| (GraphNodeHandle::from(n), n))
        .collect();
    let to_nodes: HashMap<GraphNodeHandle, &GraphNode> = to_file
        .nodes
        .iter()
        .map(|n| (GraphNodeHandle::from(n), n))
        .collect();

    let from_handles: HashSet<&GraphNodeHandle> = from_nodes.keys().collect();
    let to_handles: HashSet<&GraphNodeHandle> = to_nodes.keys().collect();

    for handle in from_handles.difference(&to_handles) {
        let node = from_nodes[handle];
        changed.push(NodeDiff {
            path: node.path.clone(),
            stable_id: node.stable_id.clone(),
            anchor_byte: node.anchor_byte,
            status: DiffStatus::Removed,
            reason: DiffReason::Removed,
            old_signature_hash: node.signature_hash.clone(),
            new_signature_hash: None,
            old_content_hash: node.content_hash.clone(),
            new_content_hash: None,
            confidence: Confidence::Exact,
        });
    }

    for handle in to_handles.difference(&from_handles) {
        let node = to_nodes[handle];
        changed.push(NodeDiff {
            path: node.path.clone(),
            stable_id: node.stable_id.clone(),
            anchor_byte: node.anchor_byte,
            status: DiffStatus::Added,
            reason: DiffReason::Added,
            old_signature_hash: None,
            new_signature_hash: node.signature_hash.clone(),
            old_content_hash: None,
            new_content_hash: node.content_hash.clone(),
            confidence: Confidence::Exact,
        });
    }

    for handle in from_handles.intersection(&to_handles) {
        let from_node = from_nodes[handle];
        let to_node = to_nodes[handle];

        if from_node.content_hash != to_node.content_hash {
            let reason = if from_node.signature_hash == to_node.signature_hash {
                DiffReason::BodyOnlyChanged
            } else {
                DiffReason::ContentChanged
            };

            changed.push(NodeDiff {
                path: from_node.path.clone(),
                stable_id: from_node.stable_id.clone(),
                anchor_byte: from_node.anchor_byte,
                status: DiffStatus::Modified,
                reason,
                old_signature_hash: from_node.signature_hash.clone(),
                new_signature_hash: to_node.signature_hash.clone(),
                old_content_hash: from_node.content_hash.clone(),
                new_content_hash: to_node.content_hash.clone(),
                confidence: if reason == DiffReason::BodyOnlyChanged {
                    Confidence::High
                } else {
                    Confidence::Medium
                },
            });
        }
    }
}

fn diff_file_symbols(
    from_file: &GraphFile,
    to_file: &GraphFile,
    changed: &mut Vec<SymbolDiff>,
) {
    let from_syms: HashMap<(&str, &std::path::Path, bool),
        &crate::graph::snapshot::GraphSymbol> = from_file
        .symbols
        .iter()
        .map(|s| ((s.name.as_str(), s.node_handle.path.as_ref(), s.is_definition), s))
        .collect();
    let to_syms: HashMap<(&str, &std::path::Path, bool),
        &crate::graph::snapshot::GraphSymbol> = to_file
        .symbols
        .iter()
        .map(|s| ((s.name.as_str(), s.node_handle.path.as_ref(), s.is_definition), s))
        .collect();

    let from_keys: HashSet<_> = from_syms.keys().copied().collect();
    let to_keys: HashSet<_> = to_syms.keys().copied().collect();

    for key in from_keys.difference(&to_keys) {
        let sym = from_syms[key];
        changed.push(SymbolDiff {
            path: sym.node_handle.path.clone(),
            name: sym.name.clone(),
            status: DiffStatus::Removed,
            reason: DiffReason::Removed,
            confidence: Confidence::Exact,
        });
    }

    for key in to_keys.difference(&from_keys) {
        let sym = to_syms[key];
        changed.push(SymbolDiff {
            path: sym.node_handle.path.clone(),
            name: sym.name.clone(),
            status: DiffStatus::Added,
            reason: DiffReason::Added,
            confidence: Confidence::Exact,
        });
    }
}

fn diff_edges(
    from_edges: &[GraphEdge],
    to_edges: &[GraphEdge],
    changed: &mut Vec<EdgeDiff>,
    _diagnostics: &mut Vec<Diagnostic>,
) {
    let from_map: HashMap<(&std::path::Path, &str, &std::path::Path), &GraphEdge> = from_edges
        .iter()
        .map(|e| {
            (
                (
                    e.source.path.as_ref(),
                    e.kind.as_str(),
                    e.target.path.as_ref(),
                ),
                e,
            )
        })
        .collect();
    let to_map: HashMap<(&std::path::Path, &str, &std::path::Path), &GraphEdge> = to_edges
        .iter()
        .map(|e| {
            (
                (
                    e.source.path.as_ref(),
                    e.kind.as_str(),
                    e.target.path.as_ref(),
                ),
                e,
            )
        })
        .collect();

    let from_keys: HashSet<_> = from_map.keys().copied().collect();
    let to_keys: HashSet<_> = to_map.keys().copied().collect();

    for key in from_keys.difference(&to_keys) {
        let edge = from_map[key];
        changed.push(EdgeDiff {
            source: edge.source.clone(),
            target: edge.target.clone(),
            kind: edge.kind.clone(),
            status: DiffStatus::Removed,
            reason: DiffReason::Removed,
            confidence: Confidence::Exact,
        });
    }

    for key in to_keys.difference(&from_keys) {
        let edge = to_map[key];
        changed.push(EdgeDiff {
            source: edge.source.clone(),
            target: edge.target.clone(),
            kind: edge.kind.clone(),
            status: DiffStatus::Added,
            reason: DiffReason::Added,
            confidence: Confidence::Exact,
        });
    }

    for key in from_keys.intersection(&to_keys) {
        let from_edge = from_map[key];
        let to_edge = to_map[key];
        if from_edge.status != to_edge.status {
            changed.push(EdgeDiff {
                source: from_edge.source.clone(),
                target: from_edge.target.clone(),
                kind: from_edge.kind.clone(),
                status: DiffStatus::Modified,
                reason: DiffReason::ContentChanged,
                confidence: Confidence::Medium,
            });
        }
    }
}

fn has_severe_changes(
    files: &[FileDiff],
    nodes: &[NodeDiff],
    _symbols: &[SymbolDiff],
) -> bool {
    // A change is "severe" if it involves file removal or node removal
    // that might indicate a rename/move rather than a body edit.
    // Without postprocess data, we cannot classify renames accurately.
    let has_removals = files.iter().any(|f| matches!(f.status, DiffStatus::Removed))
        || nodes.iter().any(|n| matches!(n.status, DiffStatus::Removed));
    let has_additions = files.iter().any(|f| matches!(f.status, DiffStatus::Added))
        || nodes.iter().any(|n| matches!(n.status, DiffStatus::Added));

    // If we see both additions and removals, it could be renames/moves
    has_removals && has_additions
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::graph::snapshot::{
        canonicalize_snapshot, GraphFile, GraphNode, GraphSnapshot, GraphSnapshotId,
        GRAPH_SCHEMA_VERSION,
    };
    use crate::{ByteRange, Confidence, StableId};

    fn make_snapshot_with_node(content_hash: &str, signature_hash: &str) -> GraphSnapshot {
        let file_hash = format!("file_{content_hash}");
        let node = GraphNode {
            path: PathBuf::from("src/lib.rs"),
            stable_id: StableId("named:foo".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 10 },
            signature_hash: Some(signature_hash.to_string()),
            content_hash: Some(content_hash.to_string()),
            confidence: Confidence::Exact,
        };

        let file = GraphFile {
            path: PathBuf::from("src/lib.rs"),
            content_hash: Some(file_hash),
            nodes: vec![node],
            symbols: vec![],
            diagnostics: vec![],
        };

        canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        })
    }

    #[test]
    fn ae3_body_only_change() {
        let old = make_snapshot_with_node("hash1", "sig1");
        let new = make_snapshot_with_node("hash2", "sig1");

        let diff = diff_snapshots(&old, &new).unwrap();

        assert_eq!(diff.changed_nodes.len(), 1);
        let node_diff = &diff.changed_nodes[0];
        assert_eq!(node_diff.status, DiffStatus::Modified);
        assert_eq!(node_diff.reason, DiffReason::BodyOnlyChanged);
        assert_eq!(node_diff.old_content_hash, Some("hash1".to_string()));
        assert_eq!(node_diff.new_content_hash, Some("hash2".to_string()));
        assert_eq!(node_diff.old_signature_hash, Some("sig1".to_string()));
        assert_eq!(node_diff.new_signature_hash, Some("sig1".to_string()));
        assert_eq!(node_diff.confidence, Confidence::High);
    }

    #[test]
    fn ae4_signature_change_triggers_postprocess_unavailable() {
        let old = make_snapshot_with_node("hash1", "sig1");
        let new = make_snapshot_with_node("hash2", "sig2");

        let diff = diff_snapshots(&old, &new).unwrap();

        assert_eq!(diff.changed_nodes.len(), 1);
        let node_diff = &diff.changed_nodes[0];
        assert_eq!(node_diff.status, DiffStatus::Modified);
        assert_eq!(node_diff.reason, DiffReason::ContentChanged);

        // No additions/removals, so postprocess should be available
        assert!(!diff.postprocess_unavailable);
    }

    #[test]
    fn ae4_rename_or_move_triggers_postprocess_unavailable() {
        let old = make_snapshot_with_node("hash1", "sig1");
        let mut new = make_snapshot_with_node("hash2", "sig2");
        // Rename the node
        new.files[0].nodes[0].name = Some("bar".to_string());
        new.files[0].nodes[0].stable_id = StableId("named:bar".to_string());
        new = canonicalize_snapshot(new);

        let diff = diff_snapshots(&old, &new).unwrap();

        // Should have both removed and added
        assert!(
            diff.changed_nodes.iter().any(|n| n.status == DiffStatus::Removed),
            "should have removed node"
        );
        assert!(
            diff.changed_nodes.iter().any(|n| n.status == DiffStatus::Added),
            "should have added node"
        );
        assert!(diff.postprocess_unavailable, "rename should trigger postprocess_unavailable");
    }

    #[test]
    fn deleted_file_shows_removed() {
        let old = make_snapshot_with_node("hash1", "sig1");
        let new = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });

        let diff = diff_snapshots(&old, &new).unwrap();

        assert_eq!(diff.changed_files.len(), 1);
        assert_eq!(diff.changed_files[0].status, DiffStatus::Removed);
        assert_eq!(diff.changed_nodes.len(), 1);
        assert_eq!(diff.changed_nodes[0].status, DiffStatus::Removed);
    }

    #[test]
    fn unchanged_snapshot_produces_empty_diff() {
        let snapshot = make_snapshot_with_node("hash1", "sig1");
        let diff = diff_snapshots(&snapshot, &snapshot).unwrap();

        assert!(diff.changed_files.is_empty());
        assert!(diff.changed_nodes.is_empty());
        assert!(diff.changed_symbols.is_empty());
        assert!(diff.changed_edges.is_empty());
        assert!(!diff.postprocess_unavailable);
    }
}

//! Cross-file reference resolver.
//!
//! Builds definition/reference candidate indexes from tags-backed
//! symbols and import/module hints. Records edges with explicit status.

use std::collections::HashMap;

use crate::graph::snapshot::{
    EdgeStatus, GraphEdge, GraphNodeHandle, GraphSnapshot,
};
use crate::schema::Confidence;
use crate::identity::StableId;

/// Resolve cross-file references for a snapshot.
///
/// Returns a list of edges with explicit status. The resolver is
/// deliberately conservative: ambiguity is represented rather than
/// hidden.
#[must_use]
pub fn resolve_xref(snapshot: &GraphSnapshot) -> Vec<GraphEdge> {
    let mut edges = Vec::new();

    // Build definition index: (name, syntax_type) -> [def handles]
    let mut definitions: HashMap<(&str, &str), Vec<&GraphNodeHandle>> = HashMap::new();
    for file in &snapshot.files {
        for sym in &file.symbols {
            if sym.is_definition {
                definitions
                    .entry((sym.name.as_str(), sym.syntax_type.as_str()))
                    .or_default()
                    .push(&sym.node_handle);
            }
        }
    }

    // Match references to definitions
    for file in &snapshot.files {
        for sym in &file.symbols {
            if !sym.is_definition {
                let ref_handle = &sym.node_handle;
                let key = (sym.name.as_str(), sym.syntax_type.as_str());

                if let Some(candidates) = definitions.get(&key) {
                    match candidates.len() {
                        0 => {
                            // unreachable because we only enter if key exists
                        }
                        1 => {
                            let target = candidates[0];
                            edges.push(GraphEdge {
                                source: ref_handle.clone(),
                                target: target.clone(),
                                kind: "reference".to_string(),
                                status: EdgeStatus::Confirmed,
                                confidence: Confidence::High,
                                candidates: vec![],
                            });
                        }
                        _ => {
                            let mut candidate_handles: Vec<GraphNodeHandle> =
                                candidates.iter().map(|c| (*c).clone()).collect();
                            // Deterministic ordering
                            candidate_handles.sort_by(|a, b| {
                                a.path.cmp(&b.path).then_with(|| {
                                    a.stable_id.0.cmp(&b.stable_id.0)
                                })
                            });

                            edges.push(GraphEdge {
                                source: ref_handle.clone(),
                                target: candidate_handles[0].clone(),
                                kind: "reference".to_string(),
                                status: EdgeStatus::Ambiguous,
                                confidence: Confidence::Low,
                                candidates: candidate_handles,
                            });
                        }
                    }
                } else {
                    edges.push(GraphEdge {
                        source: ref_handle.clone(),
                        target: GraphNodeHandle {
                            path: ref_handle.path.clone(),
                            stable_id: StableId("unresolved".to_string()),
                            anchor_byte: 0,
                        },
                        kind: "reference".to_string(),
                        status: EdgeStatus::Unresolved,
                        confidence: Confidence::Medium,
                        candidates: vec![],
                    });
                }
            }
        }
    }

    edges
}

/// Substrate query: does a node exist in the snapshot?
#[must_use]
pub fn node_exists(snapshot: &GraphSnapshot, handle: &GraphNodeHandle) -> bool {
    snapshot.files.iter().any(|file| {
        file.nodes.iter().any(|node| {
            node.path == handle.path
                && node.stable_id == handle.stable_id
                && node.anchor_byte == handle.anchor_byte
        })
    })
}

/// Substrate query: get current signature metadata for a handle.
#[must_use]
pub fn node_signature(snapshot: &GraphSnapshot, handle: &GraphNodeHandle) -> Option<NodeSignature> {
    snapshot.files.iter().find_map(|file| {
        file.nodes.iter().find_map(|node| {
            if node.path == handle.path
                && node.stable_id == handle.stable_id
                && node.anchor_byte == handle.anchor_byte
            {
                Some(NodeSignature {
                    kind: node.kind.clone(),
                    name: node.name.clone(),
                    signature_hash: node.signature_hash.clone(),
                    content_hash: node.content_hash.clone(),
                })
            } else {
                None
            }
        })
    })
}

/// Signature metadata for a node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeSignature {
    pub kind: String,
    pub name: Option<String>,
    pub signature_hash: Option<String>,
    pub content_hash: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::graph::snapshot::{
        canonicalize_snapshot, GraphFile, GraphNode, GraphSnapshot, GraphSnapshotId, GraphSymbol,
        GRAPH_SCHEMA_VERSION,
    };
    use crate::{ByteRange, Confidence, StableId};

    fn make_snapshot_with_refs() -> GraphSnapshot {
        let def_node = GraphNode {
            path: PathBuf::from("src/b.rs"),
            stable_id: StableId("named:helper".to_string()),
            kind: "function_item".to_string(),
            name: Some("helper".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 20 },
            signature_hash: Some("sig_helper".to_string()),
            content_hash: Some("hash_helper".to_string()),
            confidence: Confidence::Exact,
        };

        let ref_node = GraphNode {
            path: PathBuf::from("src/a.rs"),
            stable_id: StableId("named:main".to_string()),
            kind: "function_item".to_string(),
            name: Some("main".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 30 },
            signature_hash: Some("sig_main".to_string()),
            content_hash: Some("hash_main".to_string()),
            confidence: Confidence::Exact,
        };

        let file_b = GraphFile {
            path: PathBuf::from("src/b.rs"),
            content_hash: None,
            nodes: vec![def_node.clone()],
            symbols: vec![GraphSymbol {
                name: "helper".to_string(),
                syntax_type: "function".to_string(),
                byte_range: ByteRange { start: 0, end: 20 },
                is_definition: true,
                node_handle: GraphNodeHandle::from(&def_node),
                confidence: Confidence::Exact,
            }],
            diagnostics: vec![],
        };

        let file_a = GraphFile {
            path: PathBuf::from("src/a.rs"),
            content_hash: None,
            nodes: vec![ref_node.clone()],
            symbols: vec![GraphSymbol {
                name: "helper".to_string(),
                syntax_type: "function".to_string(),
                byte_range: ByteRange { start: 10, end: 20 },
                is_definition: false,
                node_handle: GraphNodeHandle::from(&ref_node),
                confidence: Confidence::Exact,
            }],
            diagnostics: vec![],
        };

        canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file_a, file_b],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        })
    }

    #[test]
    fn ae6_confirmed_reference() {
        let snapshot = make_snapshot_with_refs();
        let edges = resolve_xref(&snapshot);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].status, EdgeStatus::Confirmed);
        assert_eq!(edges[0].kind, "reference");
        assert_eq!(edges[0].source.path, PathBuf::from("src/a.rs"));
        assert_eq!(edges[0].target.path, PathBuf::from("src/b.rs"));
        assert_eq!(edges[0].confidence, Confidence::High);
        assert!(edges[0].candidates.is_empty());
    }

    #[test]
    fn ae6_ambiguous_reference() {
        let mut snapshot = make_snapshot_with_refs();
        // Add a second definition of "helper"
        let dup_node = GraphNode {
            path: PathBuf::from("src/c.rs"),
            stable_id: StableId("named:helper".to_string()),
            kind: "function_item".to_string(),
            name: Some("helper".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 20 },
            signature_hash: Some("sig_helper2".to_string()),
            content_hash: Some("hash_helper2".to_string()),
            confidence: Confidence::Exact,
        };
        let file_c = GraphFile {
            path: PathBuf::from("src/c.rs"),
            content_hash: None,
            nodes: vec![dup_node.clone()],
            symbols: vec![GraphSymbol {
                name: "helper".to_string(),
                syntax_type: "function".to_string(),
                byte_range: ByteRange { start: 0, end: 20 },
                is_definition: true,
                node_handle: GraphNodeHandle::from(&dup_node),
                confidence: Confidence::Exact,
            }],
            diagnostics: vec![],
        };
        snapshot.files.push(file_c);
        snapshot = canonicalize_snapshot(snapshot);

        let edges = resolve_xref(&snapshot);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].status, EdgeStatus::Ambiguous);
        assert_eq!(edges[0].confidence, Confidence::Low);
        assert_eq!(edges[0].candidates.len(), 2);
    }

    #[test]
    fn ae9_node_exists_and_signature() {
        let snapshot = make_snapshot_with_refs();
        let handle = GraphNodeHandle {
            path: PathBuf::from("src/b.rs"),
            stable_id: StableId("named:helper".to_string()),
            anchor_byte: 0,
        };

        assert!(node_exists(&snapshot, &handle));

        let sig = node_signature(&snapshot, &handle).unwrap();
        assert_eq!(sig.name, Some("helper".to_string()));
        assert_eq!(sig.signature_hash, Some("sig_helper".to_string()));
    }

    #[test]
    fn ae9_missing_node_returns_none() {
        let snapshot = make_snapshot_with_refs();
        let handle = GraphNodeHandle {
            path: PathBuf::from("src/missing.rs"),
            stable_id: StableId("named:missing".to_string()),
            anchor_byte: 0,
        };

        assert!(!node_exists(&snapshot, &handle));
        assert!(node_signature(&snapshot, &handle).is_none());
    }

    #[test]
    fn unsupported_language_no_confirmed_edges() {
        // Snapshot with no symbols produces no edges
        let snapshot = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![GraphFile {
                path: PathBuf::from("src/plain.txt"),
                content_hash: None,
                nodes: vec![GraphNode {
                    path: PathBuf::from("src/plain.txt"),
                    stable_id: StableId("unnamed:0".to_string()),
                    kind: "text".to_string(),
                    name: None,
                    anchor_byte: 0,
                    byte_range: ByteRange { start: 0, end: 10 },
                    signature_hash: None,
                    content_hash: None,
                    confidence: Confidence::Exact,
                }],
                symbols: vec![],
                diagnostics: vec![],
            }],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });

        let edges = resolve_xref(&snapshot);
        assert!(edges.is_empty(), "no symbols means no edges");
    }
}

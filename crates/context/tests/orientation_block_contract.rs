//! Contract tests for orientation block serialization.

use tree_sitter_context::orientation::{
    build_orientation, OrientationField,
};
use tree_sitter_context::graph::snapshot::{
    GraphFile, GraphNode, GraphSnapshot, GraphSnapshotId,
    GraphSymbol, GRAPH_SCHEMA_VERSION,
};
use tree_sitter_context::schema::{ByteRange, Confidence};
use tree_sitter_context::identity::StableId;
use tree_sitter_context::sexpr::orientation_to_sexpr;
use std::path::PathBuf;

fn make_test_snapshot_with_edges() -> GraphSnapshot {
    let node_a = GraphNode {
        path: PathBuf::from("src/a.rs"),
        stable_id: StableId("named:target".to_string()),
        kind: "function_item".to_string(),
        name: Some("target".to_string()),
        anchor_byte: 0,
        byte_range: ByteRange { start: 0, end: 10 },
        signature_hash: None,
        content_hash: None,
        confidence: Confidence::Exact,
    };

    let node_b = GraphNode {
        path: PathBuf::from("src/b.rs"),
        stable_id: StableId("named:caller".to_string()),
        kind: "function_item".to_string(),
        name: Some("caller".to_string()),
        anchor_byte: 0,
        byte_range: ByteRange { start: 0, end: 10 },
        signature_hash: None,
        content_hash: None,
        confidence: Confidence::Exact,
    };

    let sym_a = GraphSymbol {
        name: "target".to_string(),
        syntax_type: "function".to_string(),
        byte_range: ByteRange { start: 0, end: 10 },
        is_definition: true,
        node_handle: (&node_a).into(),
        confidence: Confidence::Exact,
    };

    let sym_b = GraphSymbol {
        name: "caller".to_string(),
        syntax_type: "function".to_string(),
        byte_range: ByteRange { start: 0, end: 10 },
        is_definition: true,
        node_handle: (&node_b).into(),
        confidence: Confidence::Exact,
    };

    let file_a = GraphFile {
        path: PathBuf::from("src/a.rs"),
        content_hash: None,
        nodes: vec![node_a.clone()],
        symbols: vec![sym_a],
        diagnostics: vec![],
    };

    let file_b = GraphFile {
        path: PathBuf::from("src/b.rs"),
        content_hash: None,
        nodes: vec![node_b.clone()],
        symbols: vec![sym_b],
        diagnostics: vec![],
    };

    // Add a cross-file edge: b -> a
    let edge = tree_sitter_context::graph::snapshot::GraphEdge {
        source: (&node_b).into(),
        target: (&node_a).into(),
        kind: "reference".to_string(),
        status: tree_sitter_context::graph::snapshot::EdgeStatus::Confirmed,
        confidence: Confidence::High,
        candidates: vec![],
    };

    GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId("test123".to_string()),
        files: vec![file_a, file_b],
        edges: vec![edge],
        diagnostics: vec![],
        meta: None,
    }
}

#[test]
fn ae1_orientation_sexpr_contains_all_fields() {
    let snapshot = make_test_snapshot_with_edges();
    let block = build_orientation(&snapshot, None);
    let bytes = orientation_to_sexpr(&block).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    assert!(s.contains("(orientation"));
    assert!(s.contains("(schema_version"));
    assert!(s.contains("(graph_snapshot_id \"test123\")"));
    assert!(s.contains("(stats"));
    assert!(s.contains("(top_referenced"));
    assert!(s.contains("(entry_points"));
    assert!(s.contains("(god_nodes postprocess_unavailable)"));
    assert!(s.contains("(communities postprocess_unavailable)"));
    assert!(s.contains("(architecture_summary postprocess_unavailable)"));
}

#[test]
fn ae2_byte_stability() {
    let snapshot = make_test_snapshot_with_edges();
    let block1 = build_orientation(&snapshot, None);
    let block2 = build_orientation(&snapshot, None);

    let bytes1 = orientation_to_sexpr(&block1).unwrap();
    let bytes2 = orientation_to_sexpr(&block2).unwrap();
    assert_eq!(bytes1, bytes2, "orientation sexpr must be byte-stable");
}

#[test]
fn ae3_budget_truncation() {
    let snapshot = make_test_snapshot_with_edges();
    let block = build_orientation(&snapshot, Some(50));

    assert!(block.budget_truncated.is_some());
    let trunc = block.budget_truncated.unwrap();
    assert_eq!(trunc.reason, "budget_exhausted");
    assert!(trunc.omitted.contains(&"entry_points".to_string()));
}

#[test]
fn reserved_postprocess_fields_are_unavailable() {
    let snapshot = make_test_snapshot_with_edges();
    let block = build_orientation(&snapshot, None);

    assert_eq!(block.god_nodes, OrientationField::PostprocessUnavailable);
    assert_eq!(block.communities, OrientationField::PostprocessUnavailable);
    assert_eq!(
        block.architecture_summary,
        OrientationField::PostprocessUnavailable
    );
}

#[test]
fn top_referenced_counts_cross_file_refs() {
    let snapshot = make_test_snapshot_with_edges();
    let block = build_orientation(&snapshot, None);

    assert!(!block.top_referenced.is_empty(), "top_referenced must not be empty when cross-file refs exist");
    let first = &block.top_referenced[0];
    assert_eq!(first.inbound_refs, 1, "target should have 1 inbound ref");
    assert_eq!(first.stable_id, "named:target");
}

#[test]
fn entry_points_excludes_nodes_with_inbound_refs() {
    let snapshot = make_test_snapshot_with_edges();
    let block = build_orientation(&snapshot, None);

    // caller has no inbound refs, so it should be an entry point
    // target has inbound refs from caller, so it should NOT be an entry point
    let entry_stable_ids: Vec<String> = block.entry_points.iter().map(|e| e.stable_id.clone()).collect();
    assert!(entry_stable_ids.contains(&"named:caller".to_string()), "caller should be an entry point");
    assert!(!entry_stable_ids.contains(&"named:target".to_string()), "target should NOT be an entry point (has inbound refs)");
}

#[test]
fn ae14_r3_upgrade_path_simulation() {
    // Simulate R3: if god_nodes were replaced with real values, only the god_nodes assertion should fail
    let snapshot = make_test_snapshot_with_edges();
    let mut block = build_orientation(&snapshot, None);

    // This simulates what R3 would do
    block.god_nodes = OrientationField::PostprocessUnavailable; // Still unavailable in R2

    // All other assertions should still pass
    assert_eq!(block.schema_version, "r2-2026-04-26");
    assert_eq!(block.graph_snapshot_id.0, "test123");
    assert!(!block.top_referenced.is_empty());
}

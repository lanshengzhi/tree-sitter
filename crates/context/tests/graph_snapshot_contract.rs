//! Graph snapshot contract tests.
//!
//! Covers deterministic identity, schema versioning, and path portability.

use std::path::PathBuf;

use tree_sitter_context::{
    ByteRange, Confidence, GraphFile, GraphMeta, GraphNode, GraphSnapshot, GraphSnapshotId,
    StableId, canonicalize_snapshot, GRAPH_SCHEMA_VERSION,
};

#[test]
fn ae2_deterministic_snapshot_id() {
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

    let s1 = canonicalize_snapshot(GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: vec![file.clone()],
        edges: vec![],
        diagnostics: vec![],
        meta: None,
    });

    let s2 = canonicalize_snapshot(GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: vec![file],
        edges: vec![],
        diagnostics: vec![],
        meta: None,
    });

    assert_eq!(s1.snapshot_id, s2.snapshot_id, "same graph must produce same id");
}

#[test]
fn ae2_insertion_order_independence() {
    let n1 = GraphNode {
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
    let n2 = GraphNode {
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

    let fa = GraphFile {
        path: PathBuf::from("src/a.rs"),
        content_hash: None,
        nodes: vec![n1.clone()],
        symbols: vec![],
        diagnostics: vec![],
    };
    let fb = GraphFile {
        path: PathBuf::from("src/b.rs"),
        content_hash: None,
        nodes: vec![n2.clone()],
        symbols: vec![],
        diagnostics: vec![],
    };

    let forward = canonicalize_snapshot(GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: vec![fa.clone(), fb.clone()],
        edges: vec![],
        diagnostics: vec![],
        meta: None,
    });

    let reverse = canonicalize_snapshot(GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: vec![fb, fa],
        edges: vec![],
        diagnostics: vec![],
        meta: None,
    });

    assert_eq!(forward.snapshot_id, reverse.snapshot_id);
}

#[test]
fn ae5_schema_mismatch_changes_id() {
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

    let s1 = canonicalize_snapshot(GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: vec![file.clone()],
        edges: vec![],
        diagnostics: vec![],
        meta: None,
    });

    let s2 = canonicalize_snapshot(GraphSnapshot {
        schema_version: "r1-9999-01-01".to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: vec![file],
        edges: vec![],
        diagnostics: vec![],
        meta: None,
    });

    assert_ne!(s1.snapshot_id, s2.snapshot_id, "different schema must produce different id");
}

#[test]
fn duplicate_stable_ids_preserved() {
    let n1 = GraphNode {
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
    let n2 = GraphNode {
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
        nodes: vec![n1, n2],
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
fn meta_does_not_affect_identity() {
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

    let without_meta = canonicalize_snapshot(GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: vec![file.clone()],
        edges: vec![],
        diagnostics: vec![],
        meta: None,
    });

    let with_meta = canonicalize_snapshot(GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: vec![file],
        edges: vec![],
        diagnostics: vec![],
        meta: Some(GraphMeta {
            created_at: Some("2026-04-26T12:00:00Z".to_string()),
            repo_root: Some(PathBuf::from("/different/path")),
            total_files: 1,
            total_nodes: 1,
            total_edges: 0,
        }),
    });

    assert_eq!(without_meta.snapshot_id, with_meta.snapshot_id);
}

//! S-expression contract tests for deterministic serialization.

use tree_sitter_context::protocol::{
    AmbiguousStableId, AstCell, Bundle, BundleResult, Candidate, Confidence, Exhausted, NotFound,
    OmittedChunk, Provenance,
};
use tree_sitter_context::sexpr::serialize;

#[test]
fn ae1_deterministic_bytes_across_100_serializations() {
    let bundle = Bundle {
        version: 1,
        path: "src/lib.rs".into(),
        cells: vec![AstCell {
            stable_id: "named:abc123".to_string(),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            byte_range: (0, 23),
            estimated_tokens: 6,
            confidence: Confidence::Exact,
        }],
        omitted: vec![],
        provenance: Provenance::new("sig_tier_bundle", Confidence::Exact),
    };

    let result = BundleResult::Bundle(bundle);
    let first = serialize(&result).unwrap();

    for _ in 0..99 {
        let next = serialize(&result).unwrap();
        assert_eq!(
            first, next,
            "serialization must be deterministic across repeated calls"
        );
    }
}

#[test]
fn ae2_not_found_has_zero_confidence_and_unknown_provenance() {
    let not_found = NotFound {
        path: "src/lib.rs".into(),
        stable_id: "named:missing".to_string(),
        reason: "no chunk with this stable_id found in file".to_string(),
        provenance: Provenance::new("stable_id_lookup", Confidence::Low),
    };

    let bytes = serialize(&BundleResult::NotFound(not_found)).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    assert!(s.contains("(not_found"));
    assert!(s.contains("(confidence low)"));
    assert!(s.contains("(graph_snapshot_id \"unknown\")"));
    assert!(s.contains("(orientation_freshness \"unknown\")"));
}

#[test]
fn ae9_exhausted_serializes_omitted_stable_ids_in_canonical_order() {
    let exhausted = Exhausted {
        path: "src/lib.rs".into(),
        stable_id: "named:foo".to_string(),
        omitted: vec![
            OmittedChunk {
                stable_id: "named:z".to_string(),
                reason: "over_budget".to_string(),
            },
            OmittedChunk {
                stable_id: "named:a".to_string(),
                reason: "over_budget".to_string(),
            },
        ],
        provenance: Provenance::new("sig_tier_bundle", Confidence::Exact),
    };

    let bytes = serialize(&BundleResult::Exhausted(exhausted)).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    let a_pos = s.find("(stable_id \"named:a\")").unwrap();
    let z_pos = s.find("(stable_id \"named:z\")").unwrap();
    assert!(a_pos < z_pos, "omitted stable_ids must be in canonical order");
}

#[test]
fn ae10_ambiguous_stable_id_serializes_all_candidates_in_order() {
    let ambiguous = AmbiguousStableId {
        path: "src/lib.rs".into(),
        stable_id: "named:dup".to_string(),
        candidates: vec![
            Candidate {
                anchor_byte: 45,
                kind: "function_item".to_string(),
                name: Some("foo".to_string()),
            },
            Candidate {
                anchor_byte: 0,
                kind: "function_item".to_string(),
                name: Some("foo".to_string()),
            },
        ],
        reason: "multiple chunks share this stable_id".to_string(),
        provenance: Provenance::new("stable_id_lookup", Confidence::Low),
    };

    let bytes = serialize(&BundleResult::AmbiguousStableId(ambiguous)).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    let pos0 = s.find("(anchor_byte 0)").unwrap();
    let pos45 = s.find("(anchor_byte 45)").unwrap();
    assert!(pos0 < pos45, "candidates must be sorted by anchor_byte");
}

#[test]
fn string_escaping_regression_quotes_backslash_newline_tab() {
    let bundle = Bundle {
        version: 1,
        path: "test.rs".into(),
        cells: vec![AstCell {
            stable_id: "named:test".to_string(),
            kind: "function_item".to_string(),
            name: Some("foo\"bar\\baz\nqux\t".to_string()),
            byte_range: (0, 10),
            estimated_tokens: 2,
            confidence: Confidence::Exact,
        }],
        omitted: vec![],
        provenance: Provenance::new("sig_tier_bundle", Confidence::Exact),
    };

    let bytes = serialize(&BundleResult::Bundle(bundle)).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    assert!(s.contains("foo\\\"bar\\\\baz\\nqux\\t"));
}

#[test]
fn provenance_serializes_all_fields() {
    let bundle = Bundle {
        version: 1,
        path: "test.rs".into(),
        cells: vec![],
        omitted: vec![],
        provenance: Provenance::new("sig_tier_bundle", Confidence::Exact),
    };

    let bytes = serialize(&BundleResult::Bundle(bundle)).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    assert!(s.contains("(strategy \"sig_tier_bundle\")"));
    assert!(s.contains("(confidence exact)"));
    assert!(s.contains("(graph_snapshot_id \"unknown\")"));
    assert!(s.contains("(orientation_freshness \"unknown\")"));
}

#[test]
fn serializer_is_only_rust_path_for_v1_bytes() {
    // This test documents that serialize() is the only intended path.
    // Any other code emitting v1 S-expressions should be considered a bug.
    let bundle = Bundle {
        version: 1,
        path: "test.rs".into(),
        cells: vec![AstCell {
            stable_id: "named:test".to_string(),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            byte_range: (0, 10),
            estimated_tokens: 2,
            confidence: Confidence::Exact,
        }],
        omitted: vec![],
        provenance: Provenance::new("sig_tier_bundle", Confidence::Exact),
    };

    let bytes = serialize(&BundleResult::Bundle(bundle)).unwrap();
    // Must start with opening paren and end with newline
    assert_eq!(bytes[0], b'(');
    assert_eq!(bytes[bytes.len() - 1], b'\n');
}

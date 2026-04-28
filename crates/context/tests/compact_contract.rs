//! Contract tests for compact S-expression serialization.

use std::path::PathBuf;

use tree_sitter_context::identity::StableId;
use tree_sitter_context::schema::{
    ByteRange, ChunkId, ChunkRecord, CompactChunkRecord, CompactFileResult, CompactOmittedRecord,
    CompactOutput, Confidence, Diagnostic, OutputMeta,
};
use tree_sitter_context::sexpr::compact_to_sexpr;

#[test]
fn compact_deterministic_bytes_across_serializations() {
    let output = create_test_compact_output();

    let first = compact_to_sexpr(&output).unwrap();

    for _ in 0..99 {
        let next = compact_to_sexpr(&output).unwrap();
        assert_eq!(
            first, next,
            "compact serialization must be deterministic across repeated calls"
        );
    }
}

#[test]
fn compact_empty_files_produces_valid_output() {
    let output = CompactOutput {
        files: vec![],
        original_tokens: 0,
        compacted_tokens: 0,
        omitted: vec![],
        diagnostics: vec![],
        meta: OutputMeta {
            schema_version: "0.1.0".to_string(),
            source_path: None,
            total_chunks: 0,
            total_estimated_tokens: 0,
        },
    };

    let bytes = compact_to_sexpr(&output).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    assert!(s.contains("(compaction"));
    assert!(s.contains("(files)"));
    assert!(s.contains("(original_tokens 0)"));
    assert!(s.contains("(compacted_tokens 0)"));
    assert!(s.contains("(schema_version \"0.1.0\")"));
}

#[test]
fn compact_all_preserved_no_signatures_or_omitted() {
    let chunk = create_test_chunk("foo", "named:foo", 10);
    let output = CompactOutput {
        files: vec![CompactFileResult {
            path: PathBuf::from("src/lib.rs"),
            preserved: vec![chunk],
            signatures_only: vec![],
            omitted: vec![],
            original_tokens: 10,
            compacted_tokens: 10,
        }],
        original_tokens: 10,
        compacted_tokens: 10,
        omitted: vec![],
        diagnostics: vec![],
        meta: OutputMeta {
            schema_version: "0.1.0".to_string(),
            source_path: Some(PathBuf::from("src/lib.rs")),
            total_chunks: 1,
            total_estimated_tokens: 10,
        },
    };

    let bytes = compact_to_sexpr(&output).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    assert!(s.contains("(preserved"));
    assert!(s.contains("(chunk"));
    assert!(s.contains("(stable_id \"named:foo\")"));
    assert!(s.contains("(signatures_only)"));
    assert!(!s.contains("(signature \""));
}

#[test]
fn compact_all_signatures_no_preserved() {
    let chunk = create_test_chunk("bar", "named:bar", 8);
    let output = CompactOutput {
        files: vec![CompactFileResult {
            path: PathBuf::from("src/lib.rs"),
            preserved: vec![],
            signatures_only: vec![CompactChunkRecord::SignatureOnly {
                chunk,
                signature: "fn bar(x: i32) -> String".to_string(),
            }],
            omitted: vec![],
            original_tokens: 20,
            compacted_tokens: 8,
        }],
        original_tokens: 20,
        compacted_tokens: 8,
        omitted: vec![],
        diagnostics: vec![],
        meta: OutputMeta {
            schema_version: "0.1.0".to_string(),
            source_path: Some(PathBuf::from("src/lib.rs")),
            total_chunks: 1,
            total_estimated_tokens: 20,
        },
    };

    let bytes = compact_to_sexpr(&output).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    assert!(s.contains("(preserved)"));
    assert!(s.contains("(signatures_only"));
    assert!(s.contains("(signature \"fn bar(x: i32) -> String\")"));
}

#[test]
fn compact_mixed_output_with_omitted() {
    let chunk_foo = create_test_chunk("foo", "named:foo", 10);
    let chunk_bar = create_test_chunk("bar", "named:bar", 8);
    let output = CompactOutput {
        files: vec![CompactFileResult {
            path: PathBuf::from("src/lib.rs"),
            preserved: vec![chunk_foo],
            signatures_only: vec![CompactChunkRecord::SignatureOnly {
                chunk: chunk_bar,
                signature: "fn bar()".to_string(),
            }],
            omitted: vec![CompactOmittedRecord {
                stable_id: StableId("named:baz".to_string()),
                kind: "function_item".to_string(),
                name: Some("baz".to_string()),
                reason: "budget".to_string(),
                estimated_tokens: 15,
            }],
            original_tokens: 33,
            compacted_tokens: 18,
        }],
        original_tokens: 33,
        compacted_tokens: 18,
        omitted: vec![],
        diagnostics: vec![Diagnostic::info("1 chunk(s) omitted due to budget")],
        meta: OutputMeta {
            schema_version: "0.1.0".to_string(),
            source_path: Some(PathBuf::from("src/lib.rs")),
            total_chunks: 3,
            total_estimated_tokens: 33,
        },
    };

    let bytes = compact_to_sexpr(&output).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    assert!(s.contains("(preserved"));
    assert!(s.contains("(signatures_only"));
    assert!(s.contains("(omitted"));
    assert!(s.contains("(omitted"));
    assert!(s.contains("(reason \"budget\")"));
    assert!(s.contains("(original_tokens 33)"));
    assert!(s.contains("(compacted_tokens 18)"));
}

#[test]
fn compact_sorts_by_stable_id() {
    let chunk_z = create_test_chunk("z", "named:z", 5);
    let chunk_a = create_test_chunk("a", "named:a", 5);

    let output = CompactOutput {
        files: vec![CompactFileResult {
            path: PathBuf::from("src/lib.rs"),
            preserved: vec![chunk_z.clone(), chunk_a.clone()],
            signatures_only: vec![],
            omitted: vec![],
            original_tokens: 10,
            compacted_tokens: 10,
        }],
        original_tokens: 10,
        compacted_tokens: 10,
        omitted: vec![],
        diagnostics: vec![],
        meta: OutputMeta {
            schema_version: "0.1.0".to_string(),
            source_path: None,
            total_chunks: 2,
            total_estimated_tokens: 10,
        },
    };

    let bytes = compact_to_sexpr(&output).unwrap();
    let s = String::from_utf8(bytes).unwrap();

    let a_pos = s.find("(stable_id \"named:a\")").unwrap();
    let z_pos = s.find("(stable_id \"named:z\")").unwrap();
    assert!(a_pos < z_pos, "preserved chunks must be sorted by stable_id");
}

fn create_test_chunk(name: &str, stable_id: &str, tokens: usize) -> ChunkRecord {
    ChunkRecord {
        id: ChunkId {
            path: PathBuf::from("src/lib.rs"),
            kind: "function_item".to_string(),
            name: Some(name.to_string()),
            anchor_byte: 0,
        },
        stable_id: StableId(stable_id.to_string()),
        kind: "function_item".to_string(),
        name: Some(name.to_string()),
        byte_range: ByteRange { start: 0, end: 11 },
        estimated_tokens: tokens,
        confidence: Confidence::Exact,
        depth: 0,
        parent: None,
    }
}

fn create_test_compact_output() -> CompactOutput {
    let chunk_foo = create_test_chunk("foo", "named:foo", 10);
    let chunk_bar = create_test_chunk("bar", "named:bar", 8);

    CompactOutput {
        files: vec![CompactFileResult {
            path: PathBuf::from("src/lib.rs"),
            preserved: vec![chunk_foo],
            signatures_only: vec![CompactChunkRecord::SignatureOnly {
                chunk: chunk_bar,
                signature: "fn bar()".to_string(),
            }],
            omitted: vec![],
            original_tokens: 18,
            compacted_tokens: 18,
        }],
        original_tokens: 18,
        compacted_tokens: 18,
        omitted: vec![],
        diagnostics: vec![],
        meta: OutputMeta {
            schema_version: "0.1.0".to_string(),
            source_path: Some(PathBuf::from("src/lib.rs")),
            total_chunks: 2,
            total_estimated_tokens: 18,
        },
    }
}

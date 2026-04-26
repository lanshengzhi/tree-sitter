//! Bundle contract tests for v1 bundle behavior.

use std::path::Path;

use tree_sitter::Parser;
use tree_sitter_context::{
    bundle::{BundleOptions, bundle_chunks},
    chunk::{ChunkOptions, chunks_for_tree},
};

fn rust_parser() -> Parser {
    let raw = unsafe { tree_sitter_rust::LANGUAGE.into_raw()() };
    let language: tree_sitter::Language = unsafe { std::mem::transmute(raw) };
    let mut parser = Parser::new();
    parser.set_language(&language).unwrap();
    parser
}

#[test]
fn ae4_honest_token_estimates_with_budget() {
    let mut parser = rust_parser();
    let source = b"fn huge() { let value = 1; let other = 2; let third = 3; }";
    let tree = parser.parse(source, None).unwrap();

    let chunk_output = chunks_for_tree(
        &tree,
        Path::new("test.rs"),
        source,
        &ChunkOptions {
            max_tokens: 1,
            max_chunks: 1_000,
        },
    );

    // The single chunk has a true estimate > 1
    assert_eq!(chunk_output.chunks.len(), 1);
    let true_estimate = chunk_output.chunks[0].estimated_tokens;
    assert!(true_estimate > 1, "true estimate must not be capped");

    // Bundle with budget 1 omits it but preserves true estimate
    let bundle = bundle_chunks(
        chunk_output.chunks,
        &BundleOptions {
            max_tokens: 1,
            max_chunks: 100,
        },
    );

    assert!(bundle.included.is_empty());
    assert_eq!(bundle.omitted.len(), 1);
    assert_eq!(bundle.omitted[0].chunk.estimated_tokens, true_estimate);
}

#[test]
fn ae10_duplicate_stable_ids_are_observable_in_chunks() {
    let mut parser = rust_parser();
    let source = b"
fn foo() {}
fn foo() {}
";
    let tree = parser.parse(source, None).unwrap();

    let chunk_output = chunks_for_tree(
        &tree,
        Path::new("test.rs"),
        source,
        &ChunkOptions::default(),
    );

    // Both functions should be present as separate chunks
    let foo_chunks: Vec<_> = chunk_output
        .chunks
        .iter()
        .filter(|c| c.name.as_deref() == Some("foo"))
        .collect();

    assert_eq!(
        foo_chunks.len(),
        2,
        "duplicate names must not be silently collapsed"
    );

    // Same-name chunks in the same file share a stable_id by design
    // (matching is name-based, not position-based)
    assert_eq!(
        foo_chunks[0].stable_id, foo_chunks[1].stable_id,
        "same-name chunks in the same file must share stable_id for matching purposes"
    );
}

#[test]
fn bundle_budget_interaction_with_max_tokens() {
    let mut parser = rust_parser();
    let source = b"
fn small() {}
fn medium() { let x = 1; }
fn large() { let x = 1; let y = 2; let z = 3; }
";
    let tree = parser.parse(source, None).unwrap();

    let chunk_output = chunks_for_tree(
        &tree,
        Path::new("test.rs"),
        source,
        &ChunkOptions::default(),
    );

    // Budget 500, max_tokens 5000: effective limit is 500
    let bundle = bundle_chunks(
        chunk_output.chunks,
        &BundleOptions {
            max_tokens: 500,
            max_chunks: 100,
        },
    );

    // All chunks should fit within 500 tokens
    assert!(
        bundle.total_included_tokens <= 500,
        "included tokens must not exceed budget"
    );

    // No chunk should have its estimate capped
    for om in &bundle.omitted {
        assert!(
            om.chunk.estimated_tokens > 0,
            "omitted chunk estimates must remain honest"
        );
    }
}

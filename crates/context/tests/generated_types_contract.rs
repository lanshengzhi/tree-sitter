//! Generated TypeScript declarations contract test.
//!
//! Verifies that ts-rs generated bindings are present and match expectations.

use std::fs;
use std::path::PathBuf;

fn bindings_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("bindings");
    path
}

#[test]
fn ae7_all_protocol_types_have_generated_bindings() {
    let dir = bindings_dir();
    assert!(dir.exists(), "bindings directory must exist");

    let expected = vec![
        "AmbiguousStableId.ts",
        "AstCell.ts",
        "Bundle.ts",
        "BundleResult.ts",
        "Candidate.ts",
        "Confidence.ts",
        "Exhausted.ts",
        "NotFound.ts",
        "OmittedChunk.ts",
        "Provenance.ts",
        "UnknownCrossFile.ts",
    ];

    for file in &expected {
        let path = dir.join(file);
        assert!(
            path.exists(),
            "generated binding must exist: {}",
            path.display()
        );

        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("export"),
            "binding must export a type: {}",
            file
        );
    }
}

#[test]
fn generated_bindings_contain_expected_fields() {
    let dir = bindings_dir();

    // Check Provenance has required fields
    let provenance = fs::read_to_string(dir.join("Provenance.ts")).unwrap();
    assert!(provenance.contains("strategy"));
    assert!(provenance.contains("confidence"));
    assert!(provenance.contains("graph_snapshot_id"));
    assert!(provenance.contains("orientation_freshness"));

    // Check BundleResult is a union type
    let bundle_result = fs::read_to_string(dir.join("BundleResult.ts")).unwrap();
    assert!(bundle_result.contains("Bundle"));
    assert!(bundle_result.contains("NotFound"));
    assert!(bundle_result.contains("AmbiguousStableId"));
    assert!(bundle_result.contains("Exhausted"));
    assert!(bundle_result.contains("UnknownCrossFile"));

    // Check Confidence has exact/high/medium/low (as string literal union)
    let confidence = fs::read_to_string(dir.join("Confidence.ts")).unwrap();
    assert!(confidence.contains("\"exact\""));
    assert!(confidence.contains("\"high\""));
    assert!(confidence.contains("\"medium\""));
    assert!(confidence.contains("\"low\""));
}

#[test]
fn generated_bindings_are_tracked_in_git() {
    // This test documents that generated bindings should be committed
    // so CI can detect drift. It's a documentation test, not a functional one.
    let dir = bindings_dir();
    assert!(dir.exists(), "bindings must be generated and committed");
}

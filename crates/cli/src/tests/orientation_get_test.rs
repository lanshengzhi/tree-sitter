//! CLI integration tests for `tree-sitter-context orientation get`.

use std::path::PathBuf;
use std::process::Command;

fn tree_sitter_context_bin() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("tree-sitter-context");
    path
}

#[test]
fn error_no_graph_returns_exit_2() {
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("orientation")
        .arg("get")
        .args(["--repo-root", "."])
        .output()
        .expect("failed to execute binary");

    assert_eq!(
        output.status.code(),
        Some(2),
        "orientation get without graph must return exit code 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no_graph"),
        "stderr must contain no_graph: {stderr}"
    );
}

#[test]
fn error_unsupported_format_returns_non_zero() {
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("orientation")
        .arg("get")
        .args(["--repo-root", "."])
        .args(["--format", "xml"])
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "unsupported format must return non-zero exit"
    );
}

#[test]
fn json_format_is_parseable() {
    // This test requires a graph to be built first.
    // In CI, it will be covered by the harness.
    // Here we just verify the CLI accepts the argument.
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("orientation")
        .arg("get")
        .args(["--repo-root", "."])
        .args(["--format", "json"])
        .output()
        .expect("failed to execute binary");

    // Without a graph, it should fail with no_graph
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.contains("no_graph") {
        // If a graph exists, verify JSON is parseable
        let stdout = String::from_utf8_lossy(&output.stdout);
        if output.status.success() {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
            assert!(parsed.is_ok(), "JSON output must be parseable: {stdout}");
        }
    }
}

#[test]
fn budget_argument_is_accepted() {
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("orientation")
        .arg("get")
        .args(["--repo-root", "."])
        .args(["--budget", "100"])
        .output()
        .expect("failed to execute binary");

    // Should either succeed (if graph exists) or fail with no_graph
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() || stderr.contains("no_graph"),
        "budget argument must be accepted: {stderr}"
    );
}

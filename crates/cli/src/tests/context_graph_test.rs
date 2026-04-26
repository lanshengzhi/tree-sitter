//! CLI integration tests for `tree-sitter-context graph`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn tree_sitter_context_bin() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("tree-sitter-context");
    path
}

fn fixture_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/tests/fixtures");
    path.push(name);
    path
}

#[test]
fn graph_build_without_grammar_reports_error() {
    let bin = tree_sitter_context_bin();
    let tmp = tempfile::tempdir().unwrap();

    // Create a simple file in the temp dir
    fs::write(tmp.path().join("test.rs"), b"fn main() {}").unwrap();

    let output = Command::new(&bin)
        .arg("graph")
        .arg("build")
        .args(["--repo-root", tmp.path().to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    // Without grammar, should still succeed but report no language found
    // or it might fail depending on loader behavior
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The command should either succeed with diagnostics or fail gracefully
    assert!(
        output.status.success() || stderr.contains("error:"),
        "graph build must not panic. stdout: {stdout}, stderr: {stderr}"
    );
}

#[test]
fn graph_status_on_empty_repo() {
    let bin = tree_sitter_context_bin();
    let tmp = tempfile::tempdir().unwrap();

    let output = Command::new(&bin)
        .arg("graph")
        .arg("status")
        .args(["--repo-root", tmp.path().to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    assert!(output.status.success(), "graph status must succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok") || stdout.contains("\"status\""),
        "status output should be JSON with status field: {stdout}"
    );
}

#[test]
fn graph_verify_without_head_reports_error() {
    let bin = tree_sitter_context_bin();
    let tmp = tempfile::tempdir().unwrap();

    let output = Command::new(&bin)
        .arg("graph")
        .arg("verify")
        .args(["--repo-root", tmp.path().to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    // Verify without HEAD should report error status
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("error") || stdout.contains("\"status\": \"error\""),
        "verify without HEAD should report error: {stdout}"
    );
}

#[test]
fn graph_clean_on_empty_repo() {
    let bin = tree_sitter_context_bin();
    assert!(bin.exists(), "tree-sitter-context binary must exist at {}", bin.display());
    let tmp = tempfile::tempdir().unwrap();

    let output = Command::new(&bin)
        .arg("graph")
        .arg("clean")
        .args(["--repo-root", tmp.path().to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "graph clean must succeed. stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        stderr
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok") || stdout.contains("\"status\""),
        "clean output should be JSON: {stdout}"
    );
}

#[test]
fn ae8_bundle_still_works_after_adding_graph_commands() {
    let bin = tree_sitter_context_bin();
    let fixture = fixture_path("small.rs");

    let output = Command::new(&bin)
        .arg("bundle")
        .arg(&fixture)
        .args(["--stable-id", "named:test"])
        .args(["--tier", "sig"])
        .args(["--format", "sexpr"])
        .args(["--max-tokens", "5000"])
        .args(["--budget", "500"])
        .output()
        .expect("failed to execute binary");

    // Should still return error for missing grammar (same as before)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:"),
        "bundle must still return error for missing grammar: {stderr}"
    );
}

#[test]
fn ae10_graph_does_not_require_daemon() {
    let bin = tree_sitter_context_bin();
    let tmp = tempfile::tempdir().unwrap();

    // graph build should run and exit without requiring background service
    let output = Command::new(&bin)
        .arg("graph")
        .arg("build")
        .args(["--repo-root", tmp.path().to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    // Should complete without hanging or requiring daemon
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("daemon") && !stderr.contains("background"),
        "graph build must not require daemon: {stderr}"
    );

    // Should produce JSON output or error, but not hang
    assert!(
        output.status.success() || !stderr.is_empty(),
        "graph build should complete: stdout={stdout}, stderr={stderr}"
    );
}

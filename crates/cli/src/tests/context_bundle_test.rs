//! CLI integration tests for `tree-sitter-context bundle`.
//!
//! Note: Success-path tests require compiled language grammars.
//! Full success-path coverage is in crates/context/tests/bundle_contract.rs.

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
fn error_unreadable_path_returns_non_zero() {
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("bundle")
        .arg("/nonexistent/path/file.rs")
        .args(["--stable-id", "named:test"])
        .args(["--tier", "sig"])
        .args(["--format", "sexpr"])
        .args(["--max-tokens", "5000"])
        .args(["--budget", "500"])
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "unreadable path must return non-zero exit"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:"),
        "stderr must contain error prefix"
    );
}

#[test]
fn error_unsupported_tier_returns_non_zero() {
    let bin = tree_sitter_context_bin();
    let fixture = fixture_path("small.rs");

    let output = Command::new(&bin)
        .arg("bundle")
        .arg(&fixture)
        .args(["--stable-id", "named:test"])
        .args(["--tier", "body"])
        .args(["--format", "sexpr"])
        .args(["--max-tokens", "5000"])
        .args(["--budget", "500"])
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "unsupported tier must return non-zero exit"
    );
}

#[test]
fn error_unsupported_format_returns_non_zero() {
    let bin = tree_sitter_context_bin();
    let fixture = fixture_path("small.rs");

    let output = Command::new(&bin)
        .arg("bundle")
        .arg(&fixture)
        .args(["--stable-id", "named:test"])
        .args(["--tier", "sig"])
        .args(["--format", "json"])
        .args(["--max-tokens", "5000"])
        .args(["--budget", "500"])
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "unsupported format must return non-zero exit"
    );
}

#[test]
fn cli_processes_arguments_and_returns_error_for_missing_grammar() {
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

    // Without grammar path, should fail with missing language
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:"),
        "CLI must return error for missing grammar: {stderr}"
    );
}

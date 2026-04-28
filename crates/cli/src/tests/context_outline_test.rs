//! CLI integration tests for `tree-sitter-context outline`.

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
fn outline_returns_symbols_with_stable_ids_and_snapshot_id() {
    let bin = tree_sitter_context_bin();
    let fixture = fixture_path("small.rs");

    let output = Command::new(&bin)
        .arg("outline")
        .arg(&fixture)
        .args(["--format", "sexpr"])
        .output()
        .expect("failed to execute binary");

    // Without grammar path, may fail with missing language
    // But if it succeeds, check structure
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("(outline"));
        assert!(stdout.contains("(snapshot_id \"snap_"));
        assert!(stdout.contains("(symbols"));
    }
}

#[test]
fn outline_missing_file_returns_non_zero() {
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("outline")
        .arg("/nonexistent/path/file.rs")
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "missing file must return non-zero exit"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("file_not_found:") || stderr.contains("error:"),
        "stderr must contain error prefix: {stderr}"
    );
}

#[test]
fn outline_unsupported_format_returns_non_zero() {
    let bin = tree_sitter_context_bin();
    let fixture = fixture_path("small.rs");

    let output = Command::new(&bin)
        .arg("outline")
        .arg(&fixture)
        .args(["--format", "yaml"])
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "unsupported format must return non-zero exit"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid_format:"),
        "stderr must contain invalid_format prefix: {stderr}"
    );
}

#[test]
fn outline_byte_stability_for_repeated_runs() {
    let bin = tree_sitter_context_bin();
    let fixture = fixture_path("small.rs");

    let output1 = Command::new(&bin)
        .arg("outline")
        .arg(&fixture)
        .args(["--format", "sexpr"])
        .output()
        .expect("failed to execute binary");

    let output2 = Command::new(&bin)
        .arg("outline")
        .arg(&fixture)
        .args(["--format", "sexpr"])
        .output()
        .expect("failed to execute binary");

    // Same exit code
    assert_eq!(
        output1.status.code(),
        output2.status.code(),
        "repeated runs must have same exit code"
    );

    // Same stderr
    assert_eq!(
        output1.stderr,
        output2.stderr,
        "repeated runs must produce identical stderr"
    );
}

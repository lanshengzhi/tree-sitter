//! CLI integration tests for `tree-sitter-context compact`.

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
fn error_missing_old_flag_returns_non_zero() {
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("compact")
        .arg(fixture_path("new_compact.rs"))
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "missing --old flag must return non-zero exit"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--old") && stderr.contains("required"),
        "stderr must indicate --old is required: {stderr}"
    );
}

#[test]
fn error_missing_new_file_returns_non_zero() {
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("compact")
        .arg("/nonexistent/path/file.rs")
        .args(["--old", "/nonexistent/path/old"])
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "missing new file must return non-zero exit"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("file_not_found:") || stderr.contains("error:"),
        "stderr must contain error prefix: {stderr}"
    );
}

#[test]
fn error_unsupported_format_returns_non_zero() {
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("compact")
        .arg(fixture_path("new_compact.rs"))
        .args(["--old", fixture_path("old_compact").to_str().unwrap()])
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
fn cli_processes_arguments_and_returns_error_for_missing_grammar() {
    let bin = tree_sitter_context_bin();
    let fixture = fixture_path("new_compact.rs");

    let output = Command::new(&bin)
        .arg("compact")
        .arg(&fixture)
        .args(["--old", fixture_path("old_compact").to_str().unwrap()])
        .args(["--format", "sexpr"])
        .output()
        .expect("failed to execute binary");

    // Without grammar path, should fail with missing language
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no_language:") || stderr.contains("error:"),
        "CLI must return error for missing grammar: {stderr}"
    );
}

#[test]
fn byte_stability_for_repeated_runs() {
    let bin = tree_sitter_context_bin();
    let fixture = fixture_path("new_compact.rs");

    // Both runs should produce identical error (missing grammar)
    let output1 = Command::new(&bin)
        .arg("compact")
        .arg(&fixture)
        .args(["--old", fixture_path("old_compact").to_str().unwrap()])
        .args(["--format", "sexpr"])
        .output()
        .expect("failed to execute binary");

    let output2 = Command::new(&bin)
        .arg("compact")
        .arg(&fixture)
        .args(["--old", fixture_path("old_compact").to_str().unwrap()])
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

#[test]
fn budget_flag_is_accepted() {
    let bin = tree_sitter_context_bin();
    let fixture = fixture_path("new_compact.rs");

    let output = Command::new(&bin)
        .arg("compact")
        .arg(&fixture)
        .args(["--old", fixture_path("old_compact").to_str().unwrap()])
        .args(["--budget", "1000"])
        .args(["--format", "sexpr"])
        .output()
        .expect("failed to execute binary");

    // Should either succeed or fail with budget_exceeded or no_language
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // The --budget flag should be parsed without error
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("Found argument"),
        "--budget flag must be accepted: {stderr}"
    );
}

#[test]
fn multiple_files_are_accepted() {
    let bin = tree_sitter_context_bin();

    let output = Command::new(&bin)
        .arg("compact")
        .arg(fixture_path("new_compact.rs"))
        .arg(fixture_path("small.rs"))
        .args(["--old", fixture_path("old_compact").to_str().unwrap()])
        .args(["--format", "sexpr"])
        .output()
        .expect("failed to execute binary");

    // Multiple files should be accepted as arguments
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("Found argument"),
        "multiple files must be accepted: {stderr}"
    );
}

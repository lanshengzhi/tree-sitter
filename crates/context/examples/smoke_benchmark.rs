//! Smoke benchmark for tree-sitter-context.
//!
//! Run with:
//!   `cargo run -p tree-sitter-context --bin smoke_benchmark`

use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use tree_sitter::Parser;
use tree_sitter_context::{
    chunk::{ChunkOptions, chunks_for_tree},
    invalidation::invalidate_snapshot,
};

fn main() {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/fixtures");

    let mut report = BenchmarkReport::new();

    for name in ["small.rs", "medium.rs"] {
        let path = fixtures_dir.join(name);
        if path.exists() {
            report.run(&path);
        } else {
            eprintln!("warning: fixture not found: {}", path.display());
        }
    }

    // Run invalidation benchmark if edited fixture exists.
    let medium = fixtures_dir.join("medium.rs");
    let medium_edited = fixtures_dir.join("medium_edited.rs");
    if medium.exists() && medium_edited.exists() {
        report.run_invalidation(&medium, &medium_edited);
    }

    println!("{}", report.to_markdown());
}

struct BenchmarkReport {
    rows: Vec<BenchmarkRow>,
    invalidation_rows: Vec<InvalidationRow>,
}

struct BenchmarkRow {
    fixture: String,
    file_size_bytes: usize,
    parse_ms: f64,
    chunk_ms: f64,
    total_ms: f64,
    chunk_count: usize,
    estimated_tokens: usize,
    json_size_bytes: usize,
    raw_source_size_bytes: usize,
}

struct InvalidationRow {
    fixture: String,
    old_size: usize,
    new_size: usize,
    diff_ms: f64,
    affected: usize,
    added: usize,
    removed: usize,
    unchanged: usize,
}

impl BenchmarkReport {
    const fn new() -> Self {
        Self {
            rows: Vec::new(),
            invalidation_rows: Vec::new(),
        }
    }

    fn run(&mut self, path: &Path) {
        let source = fs::read(path).expect("read fixture");
        let raw_source_size = source.len();

        let raw = unsafe { tree_sitter_rust::LANGUAGE.into_raw()() };
        let language: tree_sitter::Language = unsafe { std::mem::transmute(raw) };
        let mut parser = Parser::new();
        parser.set_language(&language).expect("set language");

        let parse_start = Instant::now();
        let tree = parser.parse(&source, None).expect("parse fixture");
        let parse_elapsed = parse_start.elapsed().as_secs_f64() * 1000.0;

        let chunk_start = Instant::now();
        let options = ChunkOptions::default();
        let chunk_result = chunks_for_tree(&tree, path, &source, &options);
        let chunk_elapsed = chunk_start.elapsed().as_secs_f64() * 1000.0;

        let total_elapsed = parse_elapsed + chunk_elapsed;
        let chunk_count = chunk_result.chunks.len();
        let estimated_tokens: usize = chunk_result.chunks.iter().map(|c| c.estimated_tokens).sum();

        let output = {
            let mut out =
                tree_sitter_context::schema::ContextOutput::new("0.1.0").with_source_path(path);
            for chunk in chunk_result.chunks {
                out.push_chunk(chunk);
            }
            for diagnostic in chunk_result.diagnostics {
                out.push_diagnostic(diagnostic);
            }
            out
        };
        let json = serde_json::to_string_pretty(&output).expect("serialize");
        let json_size = json.len();

        self.rows.push(BenchmarkRow {
            fixture: path.file_name().unwrap().to_string_lossy().to_string(),
            file_size_bytes: source.len(),
            parse_ms: parse_elapsed,
            chunk_ms: chunk_elapsed,
            total_ms: total_elapsed,
            chunk_count,
            estimated_tokens,
            json_size_bytes: json_size,
            raw_source_size_bytes: raw_source_size,
        });
    }

    fn run_invalidation(&mut self, old_path: &Path, new_path: &Path) {
        let old_source = fs::read(old_path).expect("read old fixture");
        let new_source = fs::read(new_path).expect("read new fixture");

        let raw = unsafe { tree_sitter_rust::LANGUAGE.into_raw()() };
        let language: tree_sitter::Language = unsafe { std::mem::transmute(raw) };
        let mut parser = Parser::new();
        parser.set_language(&language).expect("set language");

        let old_tree = parser.parse(&old_source, None).expect("parse old");
        let new_tree = parser.parse(&new_source, None).expect("parse new");

        let diff_start = Instant::now();
        let output = invalidate_snapshot(&old_tree, &new_tree, &old_source, &new_source, new_path)
            .expect("invalidate snapshot");
        let diff_elapsed = diff_start.elapsed().as_secs_f64() * 1000.0;

        self.invalidation_rows.push(InvalidationRow {
            fixture: format!(
                "{} -> {}",
                old_path.file_name().unwrap().to_string_lossy(),
                new_path.file_name().unwrap().to_string_lossy()
            ),
            old_size: old_source.len(),
            new_size: new_source.len(),
            diff_ms: diff_elapsed,
            affected: output.affected.len(),
            added: output.added.len(),
            removed: output.removed.len(),
            unchanged: output.unchanged.len(),
        });
    }

    fn to_markdown(&self) -> String {
        let mut lines = vec![
            "# tree-sitter-context Smoke Benchmark".to_string(),
            String::new(),
            "## Chunking".to_string(),
            String::new(),
            "| Fixture | File Size | Parse (ms) | Chunk (ms) | Total (ms) | Chunks | Est. Tokens | JSON Size | Raw Size |".to_string(),
            "|---------|-----------|------------|------------|------------|--------|-------------|-----------|----------|".to_string(),
        ];

        for row in &self.rows {
            lines.push(format!(
                "| {} | {} B | {:.3} | {:.3} | {:.3} | {} | {} | {} B | {} B |",
                row.fixture,
                row.file_size_bytes,
                row.parse_ms,
                row.chunk_ms,
                row.total_ms,
                row.chunk_count,
                row.estimated_tokens,
                row.json_size_bytes,
                row.raw_source_size_bytes,
            ));
        }

        lines.push(String::new());
        lines.push("## Invalidation".to_string());
        lines.push(String::new());
        lines.push(
            "| Fixture | Old Size | New Size | Diff (ms) | Affected | Added | Removed | Unchanged |".to_string(),
        );
        lines.push(
            "|---------|----------|----------|-----------|----------|-------|---------|-----------|".to_string(),
        );

        for row in &self.invalidation_rows {
            lines.push(format!(
                "| {} | {} B | {} B | {:.3} | {} | {} | {} | {} |",
                row.fixture,
                row.old_size,
                row.new_size,
                row.diff_ms,
                row.affected,
                row.added,
                row.removed,
                row.unchanged,
            ));
        }

        lines.push(String::new());
        lines.push("## Baseline Comparison".to_string());
        lines.push(String::new());

        for row in &self.rows {
            let overhead = if row.raw_source_size_bytes > 0 {
                (row.json_size_bytes as f64 / row.raw_source_size_bytes as f64) - 1.0
            } else {
                0.0
            };
            lines.push(format!(
                "- **{}**: JSON overhead vs raw source = {:.1}%, parse+chunk latency = {:.3} ms",
                row.fixture,
                overhead * 100.0,
                row.total_ms,
            ));
        }

        lines.join("\n")
    }
}

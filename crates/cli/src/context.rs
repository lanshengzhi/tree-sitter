use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use anyhow::{Context as _, Result, anyhow};
use serde::Serialize;
use tree_sitter::Parser;
use tree_sitter_context::{
    bundle::{BundleOptions, bundle_chunks},
    chunk::ChunkOptions,
    schema::{ContextOutput, Diagnostic},
};
use tree_sitter_loader::Loader;

pub struct ContextOptions {
    pub quiet: bool,
    pub old_path: Option<std::path::PathBuf>,
    pub symbols: bool,
    pub budget: Option<usize>,
}

pub fn run(loader: &Loader, path: &Path, opts: &ContextOptions) -> Result<()> {
    if let Some(output) = run_to_string(loader, path, opts)? {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        stdout.write_all(output.as_bytes())?;
    }

    Ok(())
}

pub fn run_to_string(
    loader: &Loader,
    path: &Path,
    opts: &ContextOptions,
) -> Result<Option<String>> {
    if let Some(old_path) = &opts.old_path {
        render_invalidate_snapshot(loader, old_path, path, opts)
    } else {
        render_chunks(loader, path, opts)
    }
}

fn render_chunks(loader: &Loader, path: &Path, opts: &ContextOptions) -> Result<Option<String>> {
    let source = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;

    let (language, language_config) = loader
        .language_configuration_for_file_name(path)?
        .ok_or_else(|| anyhow!("no language found for {}", path.display()))?;

    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow!("failed to parse {}", path.display()))?;

    let chunk_result =
        tree_sitter_context::chunk::chunks_for_tree(&tree, path, &source, &ChunkOptions::default());

    if let Some(max_tokens) = opts.budget {
        let mut bundle = bundle_chunks(
            chunk_result.chunks,
            &BundleOptions {
                max_tokens,
                ..Default::default()
            },
        );
        bundle.diagnostics.extend(chunk_result.diagnostics);
        if opts.symbols {
            bundle.diagnostics.push(Diagnostic::warn(
                "symbols are omitted from budgeted context output",
            ));
        }

        return render_json(&bundle, opts.quiet);
    }

    let mut output = ContextOutput::new("0.1.0").with_source_path(path);

    for chunk in chunk_result.chunks {
        output.push_chunk(chunk);
    }
    for diagnostic in chunk_result.diagnostics {
        output.push_diagnostic(diagnostic);
    }

    if opts.symbols {
        if let Some(tags_config) = language_config.tags_config(language)? {
            let symbol_opts = tree_sitter_context::symbols::SymbolOptions::default();
            let symbol_result = tree_sitter_context::symbols::symbols_for_tree(
                path,
                &source,
                tags_config,
                &symbol_opts,
            );
            for symbol in symbol_result.symbols {
                output.symbols.push(symbol);
            }
            for diagnostic in symbol_result.diagnostics {
                output.push_diagnostic(diagnostic);
            }
        } else {
            output.push_diagnostic(tree_sitter_context::schema::Diagnostic::warn(
                "no tags configuration found for this language; symbols omitted",
            ));
        }
    }

    if output.meta.total_chunks == 0 {
        output.push_diagnostic(tree_sitter_context::schema::Diagnostic::warn(
            "no chunk boundaries found; file may be empty or use an unsupported grammar",
        ));
    }

    render_json(&output, opts.quiet)
}

fn render_invalidate_snapshot(
    loader: &Loader,
    old_path: &Path,
    new_path: &Path,
    opts: &ContextOptions,
) -> Result<Option<String>> {
    let old_source =
        fs::read(old_path).with_context(|| format!("failed to read {}", old_path.display()))?;
    let new_source =
        fs::read(new_path).with_context(|| format!("failed to read {}", new_path.display()))?;

    let language = loader
        .language_configuration_for_file_name(new_path)?
        .map(|(lang, _)| lang)
        .ok_or_else(|| anyhow!("no language found for {}", new_path.display()))?;

    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let old_tree = parser
        .parse(&old_source, None)
        .ok_or_else(|| anyhow!("failed to parse {}", old_path.display()))?;
    let new_tree = parser
        .parse(&new_source, None)
        .ok_or_else(|| anyhow!("failed to parse {}", new_path.display()))?;

    let output = tree_sitter_context::invalidation::invalidate_snapshot(
        &old_tree,
        &new_tree,
        &old_source,
        &new_source,
        new_path,
    )?;

    render_json(&output, opts.quiet)
}

fn render_json(value: &impl Serialize, quiet: bool) -> Result<Option<String>> {
    if quiet {
        Ok(None)
    } else {
        Ok(Some(format!("{}\n", serde_json::to_string_pretty(value)?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter_context::{
        bundle::BundleOutput,
        identity::StableId,
        schema::{ByteRange, ChunkId, ChunkRecord, Confidence},
    };

    #[test]
    fn render_json_returns_none_when_quiet() {
        let output = ContextOutput::new("0.1.0");
        assert!(render_json(&output, true).unwrap().is_none());
    }

    #[test]
    fn render_json_pretty_prints_with_trailing_newline() {
        let mut output = ContextOutput::new("0.1.0").with_source_path("src/lib.rs");
        output.push_chunk(chunk("src/lib.rs", "function_item", Some("parse"), 10));

        let json = render_json(&output, false).unwrap().unwrap();

        assert!(json.ends_with('\n'));
        assert!(json.contains("  \"chunks\": ["));
        assert!(json.contains("\"total_chunks\": 1"));
        assert!(json.contains("\"source_path\": \"src/lib.rs\""));
    }

    #[test]
    fn render_json_preserves_budget_output_contract() {
        let output = BundleOutput {
            included: vec![chunk("src/lib.rs", "function_item", Some("small"), 8)],
            omitted: Vec::new(),
            total_included_tokens: 8,
            total_omitted_tokens: 0,
            budget: 16,
            diagnostics: vec![Diagnostic::warn(
                "symbols are omitted from budgeted context output",
            )],
        };

        let json = render_json(&output, false).unwrap().unwrap();

        assert!(json.contains("\"included\""));
        assert!(json.contains("\"budget\": 16"));
        assert!(json.contains("\"symbols are omitted from budgeted context output\""));
        assert!(!json.contains("\"chunks\""));
    }

    fn chunk(path: &str, kind: &str, name: Option<&str>, tokens: usize) -> ChunkRecord {
        ChunkRecord {
            id: ChunkId {
                path: path.into(),
                kind: kind.into(),
                name: name.map(str::to_owned),
                anchor_byte: 0,
            },
            stable_id: StableId(format!("stable:{kind}:{}", name.unwrap_or("_"))),
            kind: kind.into(),
            name: name.map(str::to_owned),
            byte_range: ByteRange { start: 0, end: 10 },
            estimated_tokens: tokens,
            confidence: Confidence::Exact,
            depth: 0,
            parent: None,
            signature_hash: "sig_hash".to_string(),
            body_hash: "body_hash".to_string(),
        }
    }
}

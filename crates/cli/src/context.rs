use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use anyhow::{Context as _, Result, anyhow};
use tree_sitter::Parser;
use tree_sitter_context::schema::ContextOutput;
use tree_sitter_loader::Loader;

pub struct ContextOptions {
    pub quiet: bool,
    pub old_path: Option<std::path::PathBuf>,
    pub symbols: bool,
    pub budget: Option<usize>,
}

pub fn run(loader: &mut Loader, path: &Path, opts: &ContextOptions) -> Result<()> {
    if let Some(old_path) = &opts.old_path {
        run_invalidate_snapshot(loader, old_path, path, opts)
    } else {
        run_chunks(loader, path, opts)
    }
}

fn run_chunks(loader: &mut Loader, path: &Path, opts: &ContextOptions) -> Result<()> {
    let source = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;

    let (language, language_config) = loader
        .language_configuration_for_file_name(path)?
        .ok_or_else(|| anyhow!("no language found for {}", path.display()))?;

    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow!("failed to parse {}", path.display()))?;

    let mut output = ContextOutput::new("0.1.0").with_source_path(path);

    let chunks =
        tree_sitter_context::chunk::chunks_for_tree(&tree, path, &source, &Default::default());
    for chunk in chunks {
        output.push_chunk(chunk);
    }

    if opts.symbols {
        if let Some(tags_config) = language_config.tags_config(language)? {
            let symbol_opts = tree_sitter_context::symbols::SymbolOptions::default();
            let symbols = tree_sitter_context::symbols::symbols_for_tree(
                &tree,
                path,
                &source,
                tags_config,
                &symbol_opts,
            );
            for symbol in symbols {
                output.symbols.push(symbol);
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

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    writeln!(&mut stdout, "{}", serde_json::to_string_pretty(&output)?)?;

    Ok(())
}

fn run_invalidate_snapshot(
    loader: &mut Loader,
    old_path: &Path,
    new_path: &Path,
    _opts: &ContextOptions,
) -> Result<()> {
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

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    writeln!(&mut stdout, "{}", serde_json::to_string_pretty(&output)?)?;

    Ok(())
}

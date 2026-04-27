//! Invalidate CLI implementation for `tree-sitter-context invalidate`.

use std::io::Write;

use anyhow::{Context as _, Result, anyhow};
use tree_sitter::Parser;
use tree_sitter_loader::Loader;

use tree_sitter_context::invalidation::invalidate_snapshot;

/// Options for the invalidate command.
pub struct InvalidateOptions {
    /// Path to the new file (current state)
    pub new_path: std::path::PathBuf,
    /// Path to the old file (previous state)
    pub old_path: std::path::PathBuf,
    /// Output format (sexpr or json)
    pub format: InvalidateFormat,
    /// Suppress main output
    pub quiet: bool,
}

/// Output format for invalidation results.
#[derive(Clone, Copy, Debug)]
pub enum InvalidateFormat {
    Sexpr,
    Json,
}

impl InvalidateFormat {
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "sexpr" => Ok(InvalidateFormat::Sexpr),
            "json" => Ok(InvalidateFormat::Json),
            _ => Err(anyhow!(
                "unsupported format: {}. Only 'sexpr' and 'json' are supported",
                s
            )),
        }
    }
}

/// Run the invalidate command comparing old and new file snapshots.
pub fn run_invalidate(opts: &InvalidateOptions) -> Result<()> {
    let loader = build_loader(None)?;
    
    // Validate paths exist
    if !opts.new_path.exists() {
        return Err(anyhow!(
            "file_not_found: new file does not exist: {}",
            opts.new_path.display()
        ));
    }
    if !opts.old_path.exists() {
        return Err(anyhow!(
            "file_not_found: old file does not exist: {}",
            opts.old_path.display()
        ));
    }

    // Read both files
    let new_source = std::fs::read(&opts.new_path)
        .with_context(|| format!("failed to read {}", opts.new_path.display()))?;
    let old_source = std::fs::read(&opts.old_path)
        .with_context(|| format!("failed to read {}", opts.old_path.display()))?;

    // Get language for the new file
    let language = loader
        .language_configuration_for_file_name(&opts.new_path)?
        .map(|(lang, _)| lang)
        .ok_or_else(|| {
            anyhow!(
                "no_language: no language grammar for {}",
                opts.new_path.display()
            )
        })?;

    // Parse both files
    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let new_tree = parser
        .parse(&new_source, None)
        .ok_or_else(|| anyhow!("parse_error: failed to parse {}", opts.new_path.display()))?;
    
    // Re-create parser for old file (tree-sitter parsers can't be reused)
    let mut parser = Parser::new();
    parser.set_language(&language)?;
    
    let old_tree = parser
        .parse(&old_source, None)
        .ok_or_else(|| anyhow!("parse_error: failed to parse {}", opts.old_path.display()))?;

    // Run invalidation
    let output = invalidate_snapshot(
        &old_tree,
        &new_tree,
        &old_source,
        &new_source,
        &opts.new_path,
    )
    .map_err(|e| anyhow!("invalidation_error: {e}"))?;

    if opts.quiet {
        return Ok(());
    }

    // Serialize output
    match opts.format {
        InvalidateFormat::Sexpr => {
            let bytes = tree_sitter_context::sexpr::invalidation_to_sexpr(&output)?;
            std::io::stdout().write_all(&bytes)?;
        }
        InvalidateFormat::Json => {
            let json = serde_json::to_string_pretty(&output)?;
            println!("{}", json);
        }
    }

    Ok(())
}

fn build_loader(grammar_path: Option<&std::path::Path>) -> Result<Loader> {
    let mut loader = if let Some(path) = grammar_path {
        Loader::with_parser_lib_path(path.to_path_buf())
    } else {
        Loader::new().map_err(|e| anyhow!("failed to create loader: {e}"))?
    };

    // Load language configurations from standard locations
    let config = tree_sitter_config::Config::load(None)?;
    let loader_config = config.get()?;
    loader.find_all_languages(&loader_config)?;

    // Load from custom grammar path if specified
    if let Some(path) = grammar_path {
        loader.find_language_configurations_at_path(path, false)?;
    }

    Ok(loader)
}
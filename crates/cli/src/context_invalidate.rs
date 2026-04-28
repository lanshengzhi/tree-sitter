//! Invalidate CLI implementation for `tree-sitter-context invalidate`.

use std::io::Write;

use anyhow::{Context as _, Result, anyhow};
use tree_sitter::Parser;
use tree_sitter_loader::Loader;

use tree_sitter_context::{
    SnapshotCache,
    chunk::{ChunkOptions, chunks_for_tree},
    invalidation::invalidate_snapshot,
};

/// Options for the invalidate command.
pub struct InvalidateOptions {
    /// Path to the new file (current state)
    pub new_path: std::path::PathBuf,
    /// Path to the old file (previous state)
    pub old_path: Option<std::path::PathBuf>,
    /// Snapshot ID to retrieve old state from cache
    pub since_snapshot_id: Option<String>,
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
    // Validate that either --old or --since-snapshot-id is provided
    if opts.old_path.is_none() && opts.since_snapshot_id.is_none() {
        return Err(anyhow!(
            "missing_required_flag: either --old or --since-snapshot-id is required"
        ));
    }

    // Validate new path exists
    if !opts.new_path.exists() {
        return Err(anyhow!(
            "file_not_found: new file does not exist: {}",
            opts.new_path.display()
        ));
    }

    let loader = build_loader(None)?;

    let new_source = std::fs::read(&opts.new_path)
        .with_context(|| format!("failed to read {}", opts.new_path.display()))?;

    let language = loader
        .language_configuration_for_file_name(&opts.new_path)?
        .map(|(lang, _)| lang)
        .ok_or_else(|| {
            anyhow!(
                "no_language: no language grammar for {}",
                opts.new_path.display()
            )
        })?;

    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let new_tree = parser
        .parse(&new_source, None)
        .ok_or_else(|| anyhow!("parse_error: failed to parse {}", opts.new_path.display()))?;

    // Resolve old source and tree
    let (old_source, old_tree, old_path) = if let Some(old_path) = &opts.old_path {
        if !old_path.exists() {
            return Err(anyhow!(
                "file_not_found: old file does not exist: {}",
                old_path.display()
            ));
        }

        let old_source = std::fs::read(old_path)
            .with_context(|| format!("failed to read {}", old_path.display()))?;

        let mut parser = Parser::new();
        parser.set_language(&language)?;

        let old_tree = parser
            .parse(&old_source, None)
            .ok_or_else(|| anyhow!("parse_error: failed to parse {}", old_path.display()))?;

        (old_source, old_tree, old_path.clone())
    } else if let Some(snapshot_id) = &opts.since_snapshot_id {
        let repo_root = resolve_repo_root(&opts.new_path).unwrap_or_else(|| std::path::PathBuf::from("."));
        let cache = SnapshotCache::open(&repo_root).map_err(|e| {
            anyhow!("cache_error: failed to open snapshot cache: {}", e)
        })?;

        let snapshot = cache
            .load(snapshot_id)?
            .ok_or_else(|| anyhow!("snapshot_not_found: snapshot {} not found in cache", snapshot_id))?;

        let old_source = std::fs::read(&snapshot.file_path)
            .with_context(|| format!("failed to read {}", snapshot.file_path.display()))?;

        let mut parser = Parser::new();
        parser.set_language(&language)?;

        let old_tree = parser
            .parse(&old_source, None)
            .ok_or_else(|| anyhow!("parse_error: failed to parse {}", snapshot.file_path.display()))?;

        (old_source, old_tree, snapshot.file_path.clone())
    } else {
        unreachable!()
    };

    // Run invalidation
    let output = invalidate_snapshot(
        &old_tree,
        &new_tree,
        &old_source,
        &new_source,
        &old_path,
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

    let config = tree_sitter_config::Config::load(None)?;
    let loader_config = config.get()?;
    loader.find_all_languages(&loader_config)?;

    if let Some(path) = grammar_path {
        loader.find_language_configurations_at_path(path, false)?;
    }

    Ok(loader)
}

/// Walk upward from `start` looking for `.tree-sitter-context-mcp/`.
fn resolve_repo_root(start: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut current = Some(start);
    while let Some(path) = current {
        if path.join(".tree-sitter-context-mcp").is_dir() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }
    None
}

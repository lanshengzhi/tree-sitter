//! Compact CLI implementation for `tree-sitter-context compact`.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use tree_sitter::Parser;
use tree_sitter_loader::Loader;
use tree_sitter_tags::TagsConfiguration;

use tree_sitter_context::compact::{CompactOptions, FileLanguage, compact_files};

/// Options for the compact command.
pub struct CompactCliOptions {
    /// Paths to new files (current state)
    pub paths: Vec<PathBuf>,
    /// Path to directory containing old file snapshots
    pub old_dir: PathBuf,
    /// Output format (sexpr or json)
    pub format: CompactFormat,
    /// Optional token budget
    pub budget: Option<usize>,
    /// Suppress main output
    pub quiet: bool,
}

/// Output format for compact results.
#[derive(Clone, Copy, Debug)]
pub enum CompactFormat {
    Sexpr,
    Json,
}

impl CompactFormat {
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "sexpr" => Ok(CompactFormat::Sexpr),
            "json" => Ok(CompactFormat::Json),
            _ => Err(anyhow!(
                "unsupported format: {}. Only 'sexpr' and 'json' are supported",
                s
            )),
        }
    }
}

/// Run the compact command comparing old and new file snapshots.
pub fn run_compact(opts: &CompactCliOptions) -> Result<()> {
    let loader = build_loader(None)?;

    // Validate paths exist
    for path in &opts.paths {
        if !path.exists() {
            return Err(anyhow!(
                "file_not_found: new file does not exist: {}",
                path.display()
            ));
        }
    }

    if !opts.old_dir.exists() {
        return Err(anyhow!(
            "file_not_found: old directory does not exist: {}",
            opts.old_dir.display()
        ));
    }

    // Read old contents
    let mut old_contents = HashMap::new();
    let mut languages = HashMap::new();
    let mut tags_configs: HashMap<PathBuf, &tree_sitter_tags::TagsConfiguration> = HashMap::new();

    for path in &opts.paths {
        let relative_path = path.strip_prefix(std::env::current_dir()?)
            .unwrap_or(path.as_path());
        let old_path = opts.old_dir.join(relative_path);

        if !old_path.exists() {
            return Err(anyhow!(
                "file_not_found: old file does not exist: {}",
                old_path.display()
            ));
        }

        let old_source = std::fs::read(&old_path)
            .with_context(|| format!("failed to read {}", old_path.display()))?;
        old_contents.insert(path.clone(), old_source);

        // Get language configuration for this file
        let (language, language_config) = loader
            .language_configuration_for_file_name(path)?
            .ok_or_else(|| {
                anyhow!(
                    "no_language: no language grammar for {}",
                    path.display()
                )
            })?;

        languages.insert(
            path.clone(),
            FileLanguage { language: language.clone() },
        );

        if let Some(tags_config) = language_config.tags_config(language)? {
            tags_configs.insert(path.clone(), tags_config);
        }
    }

    let compact_opts = CompactOptions {
        budget: opts.budget,
    };

    let output = compact_files(
        &opts.paths,
        &old_contents,
        &languages,
        &tags_configs,
        &compact_opts,
    )
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("budget_exceeded") {
            anyhow!("budget_exceeded: {}", msg)
        } else {
            anyhow!("compaction_error: {}", msg)
        }
    })?;

    if opts.quiet {
        return Ok(());
    }

    // Serialize output
    match opts.format {
        CompactFormat::Sexpr => {
            let bytes = tree_sitter_context::sexpr::compact_to_sexpr(&output)?;
            std::io::stdout().write_all(&bytes)?;
        }
        CompactFormat::Json => {
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

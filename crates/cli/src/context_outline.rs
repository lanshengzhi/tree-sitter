//! Outline CLI implementation for `tree-sitter-context outline`.

use std::io::Write;

use anyhow::{Context as _, Result, anyhow};
use tree_sitter::Parser;
use tree_sitter_loader::Loader;

use tree_sitter_context::{
    SnapshotCache,
    chunk::{ChunkOptions, chunks_for_tree},
};

/// Options for the outline command.
pub struct OutlineOptions {
    /// Path to the source file
    pub path: std::path::PathBuf,
    /// Output format (sexpr or json)
    pub format: OutlineFormat,
    /// Suppress main output
    pub quiet: bool,
    /// Custom grammar directory
    pub grammar_path: Option<std::path::PathBuf>,
}

/// Output format for outline results.
#[derive(Clone, Copy, Debug)]
pub enum OutlineFormat {
    Sexpr,
    Json,
}

impl OutlineFormat {
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "sexpr" => Ok(OutlineFormat::Sexpr),
            "json" => Ok(OutlineFormat::Json),
            _ => Err(anyhow!(
                "unsupported format: {}. Only 'sexpr' and 'json' are supported",
                s
            )),
        }
    }
}

/// Run the outline command for a source file.
pub fn run_outline(opts: &OutlineOptions) -> Result<()> {
    // Validate path exists
    if !opts.path.exists() {
        return Err(anyhow!(
            "file_not_found: file does not exist: {}",
            opts.path.display()
        ));
    }

    let source = std::fs::read(&opts.path)
        .with_context(|| format!("failed to read {}", opts.path.display()))?;

    let loader = build_loader(opts.grammar_path.as_deref())?;

    let language = loader
        .language_configuration_for_file_name(&opts.path)?
        .map(|(lang, _)| lang)
        .ok_or_else(|| {
            anyhow!(
                "no_language: no language grammar for {}",
                opts.path.display()
            )
        })?;

    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow!("parse_error: failed to parse {}", opts.path.display()))?;

    let chunk_output = chunks_for_tree(
        &tree,
        &opts.path,
        &source,
        &ChunkOptions::default(),
    );

    // Generate snapshot ID
    let snapshot_id = format!("snap_{}", uuid::Uuid::new_v4().simple());

    // Save snapshot to cache
    let repo_root = resolve_repo_root(&opts.path).unwrap_or_else(|| std::path::PathBuf::from("."));
    if let Ok(cache) = SnapshotCache::open(&repo_root) {
        if let Err(e) = cache.save(&snapshot_id, &opts.path, &chunk_output.chunks) {
            eprintln!("warning: failed to save snapshot to cache: {}", e);
        }
    }

    if opts.quiet {
        return Ok(());
    }

    // Build outline output
    let outline = OutlineOutput {
        schema_version: "0.2.0".to_string(),
        snapshot_id: snapshot_id.clone(),
        symbols: chunk_output
            .chunks
            .iter()
            .map(|chunk| OutlineSymbol {
                stable_id: chunk.stable_id.0.clone(),
                kind: chunk.kind.clone(),
                name: chunk.name.clone(),
                byte_range: (chunk.byte_range.start, chunk.byte_range.end),
                signature_hash: chunk.signature_hash.clone(),
                body_hash: chunk.body_hash.clone(),
            })
            .collect(),
        diagnostics: chunk_output.diagnostics,
        meta: tree_sitter_context::schema::OutputMeta {
            schema_version: "0.2.0".to_string(),
            source_path: Some(opts.path.clone()),
            total_chunks: chunk_output.chunks.len(),
            total_estimated_tokens: chunk_output
                .chunks
                .iter()
                .map(|c| c.estimated_tokens)
                .sum(),
        },
    };

    // Serialize output
    match opts.format {
        OutlineFormat::Sexpr => {
            let bytes = outline_to_sexpr(&outline)?;
            std::io::stdout().write_all(&bytes)?;
        }
        OutlineFormat::Json => {
            let json = serde_json::to_string_pretty(&outline)?;
            println!("{}", json);
        }
    }

    Ok(())
}

/// Outline output structure.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OutlineOutput {
    pub schema_version: String,
    pub snapshot_id: String,
    pub symbols: Vec<OutlineSymbol>,
    pub diagnostics: Vec<tree_sitter_context::schema::Diagnostic>,
    pub meta: tree_sitter_context::schema::OutputMeta,
}

/// A single symbol in the outline.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OutlineSymbol {
    pub stable_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub byte_range: (usize, usize),
    pub signature_hash: String,
    pub body_hash: String,
}

fn outline_to_sexpr(output: &OutlineOutput) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    outline_to_sexpr_inner(&mut buf, output, 0)?;
    buf.push(b'\n');
    Ok(buf)
}

fn outline_to_sexpr_inner(
    w: &mut impl Write,
    output: &OutlineOutput,
    depth: usize,
) -> std::io::Result<()> {
    indent(w, depth)?;
    write!(w, "(outline")?;
    write!(w, "\n")?;

    indent(w, depth + 1)?;
    write!(w, "(schema_version {})", escape_string(&output.schema_version))?;
    write!(w, "\n")?;

    indent(w, depth + 1)?;
    write!(w, "(snapshot_id {})", escape_string(&output.snapshot_id))?;
    write!(w, "\n")?;

    // Symbols
    indent(w, depth + 1)?;
    write!(w, "(symbols")?;
    if output.symbols.is_empty() {
        write!(w, ")")?;
    } else {
        let mut symbols = output.symbols.clone();
        symbols.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        for symbol in &symbols {
            write!(w, "\n")?;
            serialize_outline_symbol(w, symbol, depth + 2)?;
        }
        write!(w, "\n")?;
        indent(w, depth + 1)?;
        write!(w, ")")?;
    }
    write!(w, "\n")?;

    // Meta
    indent(w, depth + 1)?;
    write!(w, "(meta")?;
    write!(w, "\n")?;
    if let Some(path) = &output.meta.source_path {
        indent(w, depth + 2)?;
        write!(w, "(source_path {})", escape_string(&path.to_string_lossy()))?;
        write!(w, "\n")?;
    }
    indent(w, depth + 2)?;
    write!(w, "(total_symbols {})", output.meta.total_chunks)?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, ")")?;

    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn serialize_outline_symbol(
    w: &mut impl Write,
    symbol: &OutlineSymbol,
    depth: usize,
) -> std::io::Result<()> {
    indent(w, depth)?;
    write!(w, "(symbol")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(stable_id {})", escape_string(&symbol.stable_id))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(kind {})", escape_string(&symbol.kind))?;
    write!(w, "\n")?;
    if let Some(name) = &symbol.name {
        indent(w, depth + 1)?;
        write!(w, "(name {})", escape_string(name))?;
        write!(w, "\n")?;
    }
    indent(w, depth + 1)?;
    write!(w, "(byte_range {} {})", symbol.byte_range.0, symbol.byte_range.1)?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(signature_hash {})", escape_string(&symbol.signature_hash))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(body_hash {})", escape_string(&symbol.body_hash))?;
    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn indent(w: &mut impl Write, depth: usize) -> std::io::Result<()> {
    for _ in 0..depth {
        write!(w, "  ")?;
    }
    Ok(())
}

fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\t' => result.push_str("\\t"),
            c => {
                if c.is_control() {
                    result.push('\u{fffd}');
                } else {
                    result.push(c);
                }
            }
        }
    }
    result.push('"');
    result
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

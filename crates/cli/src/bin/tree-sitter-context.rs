use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow};
use clap::{Args, Parser, Subcommand};
use tree_sitter::Parser as TSParser;
use tree_sitter_context::{
    GraphStore,
    bundle::{BundleOptions, bundle_chunks},
    chunk::{ChunkOptions, chunks_for_tree},
    graph::snapshot::GraphError,
    protocol::{
        AmbiguousStableId, AstCell, Bundle, BundleResult, Candidate, Confidence, Exhausted,
        NotFound, OmittedChunk, Provenance,
    },
    sexpr::serialize,
};
use tree_sitter_loader::Loader;

#[derive(Parser)]
#[command(name = "tree-sitter-context")]
#[command(about = "Extract structured code context for LLM consumption")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract a context bundle for a specific stable ID
    Bundle(BundleArgs),
    /// Graph operations for repo map substrate
    Graph(GraphArgs),
}

#[derive(Args)]
struct GraphArgs {
    #[command(subcommand)]
    command: GraphCommands,
}

#[derive(Subcommand)]
enum GraphCommands {
    /// Build a graph snapshot from the repo
    Build(GraphBuildArgs),
    /// Incrementally update graph from previous HEAD
    Update(GraphBuildArgs),
    /// Show graph store status
    Status(GraphStatusArgs),
    /// Verify graph store integrity
    Verify(GraphVerifyArgs),
    /// Diff two snapshots
    Diff(GraphDiffArgs),
    /// Clean unreachable snapshots
    Clean(GraphCleanArgs),
}

#[derive(Args)]
struct GraphBuildArgs {
    /// Repository root to scan
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,

    /// Custom grammar directory
    #[arg(long)]
    grammar_path: Option<PathBuf>,

    /// Suppress main output
    #[arg(long, short)]
    quiet: bool,
}

#[derive(Args)]
struct GraphStatusArgs {
    /// Repository root
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,
}

#[derive(Args)]
struct GraphVerifyArgs {
    /// Repository root
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,
}

#[derive(Args)]
struct GraphDiffArgs {
    /// Repository root
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,

    /// From snapshot ID
    #[arg(long)]
    from: String,

    /// To snapshot ID
    #[arg(long)]
    to: String,
}

#[derive(Args)]
struct GraphCleanArgs {
    /// Repository root
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,
}

#[derive(Args)]
struct BundleArgs {
    /// Path to the source file
    #[arg(index = 1)]
    path: PathBuf,

    /// Stable ID to locate
    #[arg(long)]
    stable_id: String,

    /// Tier to extract (only "sig" supported in v1)
    #[arg(long, default_value = "sig")]
    tier: String,

    /// Output format (only "sexpr" supported in v1)
    #[arg(long, default_value = "sexpr")]
    format: String,

    /// Maximum tokens for the result (bridge ceiling)
    #[arg(long)]
    max_tokens: usize,

    /// Token budget for included chunks
    #[arg(long)]
    budget: usize,

    /// Custom grammar directory
    #[arg(long)]
    grammar_path: Option<PathBuf>,

    /// Suppress main output
    #[arg(long, short)]
    quiet: bool,

    /// Expected snapshot ID for freshness check
    #[arg(long)]
    orientation_snapshot_id: Option<String>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Bundle(args) => run_bundle(args),
        Commands::Graph(args) => run_graph(args),
    }
}

fn run_graph(args: GraphArgs) -> Result<()> {
    use tree_sitter_cli::context_graph;
    use tree_sitter_context::GraphSnapshotId;

    match args.command {
        GraphCommands::Build(build_args) => {
            let opts = context_graph::GraphBuildOptions {
                repo_root: build_args.repo_root,
                grammar_path: build_args.grammar_path,
                quiet: build_args.quiet,
            };
            let result = context_graph::build_graph(&opts)?;
            if !opts.quiet {
                let json = context_graph::render_json(&result)?;
                std::io::stdout().write_all(json.as_bytes())?;
            }
            Ok(())
        }
        GraphCommands::Update(build_args) => {
            let opts = context_graph::GraphBuildOptions {
                repo_root: build_args.repo_root,
                grammar_path: build_args.grammar_path,
                quiet: build_args.quiet,
            };
            let result = context_graph::graph_update(&opts)?;
            if !opts.quiet {
                let json = context_graph::render_json(&result)?;
                std::io::stdout().write_all(json.as_bytes())?;
            }
            Ok(())
        }
        GraphCommands::Status(status_args) => {
            let result = context_graph::graph_status(&status_args.repo_root)?;
            let json = context_graph::render_json(&result)?;
            std::io::stdout().write_all(json.as_bytes())?;
            Ok(())
        }
        GraphCommands::Verify(verify_args) => {
            let result = context_graph::graph_verify(&verify_args.repo_root)?;
            let json = context_graph::render_json(&result)?;
            std::io::stdout().write_all(json.as_bytes())?;
            Ok(())
        }
        GraphCommands::Diff(diff_args) => {
            let diff = context_graph::graph_diff(
                &diff_args.repo_root,
                &GraphSnapshotId(diff_args.from),
                &GraphSnapshotId(diff_args.to),
            )?;
            let json = context_graph::render_json(&diff)?;
            std::io::stdout().write_all(json.as_bytes())?;
            Ok(())
        }
        GraphCommands::Clean(clean_args) => {
            let result = context_graph::graph_clean(&clean_args.repo_root)?;
            let json = context_graph::render_json(&result)?;
            std::io::stdout().write_all(json.as_bytes())?;
            Ok(())
        }
    }
}

fn run_bundle(args: BundleArgs) -> Result<()> {
    // Validate format
    if args.format != "sexpr" {
        return Err(anyhow!(
            "unsupported format: {}. Only 'sexpr' is supported in v1",
            args.format
        ));
    }

    // Validate tier
    if args.tier != "sig" {
        return Err(anyhow!(
            "unsupported tier: {}. Only 'sig' is supported in v1",
            args.tier
        ));
    }

    // Validate path exists and is readable
    if !args.path.exists() {
        return Err(anyhow!(
            "unreadable path: {}",
            args.path.display()
        ));
    }

    // Resolve repo root and read HEAD for orientation metadata
    let (snapshot_id, freshness) =
        match resolve_repo_root(&args.path).and_then(|root| GraphStore::open(root).ok()) {
            Some(store) => match store.read_head() {
                Ok(head) => {
                    let id = head.0.clone();
                    let fresh = match &args.orientation_snapshot_id {
                        Some(expected) if expected == &id => "fresh",
                        Some(_) => "stale",
                        None => "unknown",
                    };
                    (id, fresh.to_string())
                }
                Err(GraphError::MissingSnapshot { .. }) => ("no_graph".to_string(), "unknown".to_string()),
                Err(GraphError::CorruptedSnapshot { reason, .. }) => {
                    eprintln!("graph_corrupt: {}", reason);
                    std::process::exit(3);
                }
                Err(GraphError::SchemaMismatch { expected, found }) => {
                    eprintln!("schema_mismatch: expected={}, found={}", expected, found);
                    std::process::exit(4);
                }
                Err(e) => {
                    eprintln!("graph_corrupt: {}", e);
                    std::process::exit(3);
                }
            },
            None => ("no_graph".to_string(), "unknown".to_string()),
        };

    // Validate stable_id format
    if !args.stable_id.contains(':') {
        return Err(anyhow!(
            "invalid stable_id format: {}",
            args.stable_id
        ));
    }

    let loader = build_loader(args.grammar_path.as_deref())?;

    let source = std::fs::read(&args.path)
        .with_context(|| format!("failed to read {}", args.path.display()))?;

    let (language, _language_config) = loader
        .language_configuration_for_file_name(&args.path)?
        .ok_or_else(|| anyhow!(
            "no language grammar for {}",
            args.path.display()
        ))?;

    let mut parser = TSParser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow!("failed to parse {}", args.path.display()))?;

    let chunk_output = chunks_for_tree(
        &tree,
        &args.path,
        &source,
        &ChunkOptions::default(),
    );

    // Find chunks matching the stable_id
    let matching_chunks: Vec<_> = chunk_output
        .chunks
        .iter()
        .filter(|c| c.stable_id.0 == args.stable_id)
        .collect();

    let result = match matching_chunks.len() {
        0 => BundleResult::NotFound(NotFound {
            path: args.path.clone(),
            stable_id: args.stable_id.clone(),
            reason: "no chunk with this stable_id found in file".to_string(),
            provenance: Provenance::new("stable_id_lookup", Confidence::Low)
                .with_graph_state(snapshot_id.clone(), freshness.clone()),
        }),
        1 => {
            let chunk = matching_chunks[0];
            let effective_budget = args.budget.min(args.max_tokens);

            // Check if the chunk itself exceeds the effective budget
            if chunk.estimated_tokens > effective_budget {
                BundleResult::Exhausted(Exhausted {
                    path: args.path.clone(),
                    stable_id: args.stable_id.clone(),
                    omitted: vec![OmittedChunk {
                        stable_id: args.stable_id.clone(),
                        reason: "over_budget".to_string(),
                    }],
                    provenance: Provenance::new("sig_tier_bundle", Confidence::Exact)
                        .with_graph_state(snapshot_id.clone(), freshness.clone()),
                })
            } else {
                // Bundle with the effective budget
                let bundle = bundle_chunks(
                    chunk_output.chunks.clone(),
                    &BundleOptions {
                        max_tokens: effective_budget,
                        max_chunks: 100,
                    },
                );

                // Check if our target chunk is in the included list
                let included = bundle.included.iter().any(|c| c.stable_id.0 == args.stable_id);

                if !included {
                    // The chunk was omitted due to budget constraints
                    BundleResult::Exhausted(Exhausted {
                        path: args.path.clone(),
                        stable_id: args.stable_id.clone(),
                        omitted: bundle
                            .omitted
                            .into_iter()
                            .filter(|o| o.chunk.stable_id.0 == args.stable_id)
                            .map(|o| OmittedChunk {
                                stable_id: o.chunk.stable_id.0.clone(),
                                reason: match o.reason {
                                    tree_sitter_context::bundle::OmissionReason::OverBudget => {
                                        "over_budget".to_string()
                                    }
                                    tree_sitter_context::bundle::OmissionReason::LowPriority => {
                                        "low_priority".to_string()
                                    }
                                },
                            })
                            .collect(),
                        provenance: Provenance::new("sig_tier_bundle", Confidence::Exact)
                            .with_graph_state(snapshot_id.clone(), freshness.clone()),
                    })
                } else {
                    // Build the bundle result with only the target chunk
                    let cells = vec![AstCell {
                        stable_id: chunk.stable_id.0.clone(),
                        kind: chunk.kind.clone(),
                        name: chunk.name.clone(),
                        byte_range: (chunk.byte_range.start, chunk.byte_range.end),
                        estimated_tokens: chunk.estimated_tokens,
                        confidence: match chunk.confidence {
                            tree_sitter_context::schema::Confidence::Exact => Confidence::Exact,
                            tree_sitter_context::schema::Confidence::High => Confidence::High,
                            tree_sitter_context::schema::Confidence::Medium => Confidence::Medium,
                            tree_sitter_context::schema::Confidence::Low => Confidence::Low,
                        },
                    }];

                    let omitted: Vec<OmittedChunk> = bundle
                        .omitted
                        .into_iter()
                        .map(|o| OmittedChunk {
                            stable_id: o.chunk.stable_id.0.clone(),
                            reason: match o.reason {
                                tree_sitter_context::bundle::OmissionReason::OverBudget => {
                                    "over_budget".to_string()
                                }
                                tree_sitter_context::bundle::OmissionReason::LowPriority => {
                                    "low_priority".to_string()
                                }
                            },
                        })
                        .collect();

                    BundleResult::Bundle(Bundle {
                        version: 1,
                        path: args.path.clone(),
                        cells,
                        omitted,
                        provenance: Provenance::new("sig_tier_bundle", Confidence::Exact)
                            .with_graph_state(snapshot_id.clone(), freshness.clone()),
                    })
                }
            }
        }
        _ => {
            // Multiple matches - ambiguous
            BundleResult::AmbiguousStableId(AmbiguousStableId {
                path: args.path.clone(),
                stable_id: args.stable_id.clone(),
                candidates: matching_chunks
                    .iter()
                    .map(|c| Candidate {
                        anchor_byte: c.id.anchor_byte,
                        kind: c.kind.clone(),
                        name: c.name.clone(),
                    })
                    .collect(),
                reason: "multiple chunks share this stable_id".to_string(),
                provenance: Provenance::new("stable_id_lookup", Confidence::Low)
                    .with_graph_state(snapshot_id.clone(), freshness.clone()),
            })
        }
    };

    if !args.quiet {
        let bytes = serialize(&result)?;
        std::io::stdout().write_all(&bytes)?;
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

/// Walk upward from `start` looking for `.tree-sitter-context-mcp/`.
fn resolve_repo_root(start: &std::path::Path) -> Option<&std::path::Path> {
    let mut current = Some(start);
    while let Some(path) = current {
        if path.join(".tree-sitter-context-mcp").is_dir() {
            return Some(path);
        }
        current = path.parent();
    }
    None
}

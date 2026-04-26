//! Graph CLI implementation for `tree-sitter-context graph ...`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use serde::Serialize;
use tree_sitter::Parser;
use tree_sitter_loader::Loader;
use walkdir::WalkDir;

use tree_sitter_context::{
    GraphMeta, GraphSnapshot, GraphSnapshotId, GraphStore,
    chunk::{ChunkOptions, chunks_for_tree},
    extract_graph_file,
    symbols::{SymbolOptions, symbols_for_tree},
    GRAPH_SCHEMA_VERSION, canonicalize_snapshot,
};

/// Options for graph build.
pub struct GraphBuildOptions {
    pub repo_root: PathBuf,
    pub grammar_path: Option<PathBuf>,
    pub quiet: bool,
}

/// Result of a graph build.
#[derive(Serialize)]
pub struct GraphBuildResult {
    pub status: String,
    pub snapshot_id: GraphSnapshotId,
    pub files_scanned: usize,
    pub files_included: usize,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub diagnostics: Vec<GraphCliDiagnostic>,
}

/// Result of graph status.
#[derive(Serialize)]
pub struct GraphStatusResult {
    pub status: String,
    pub head: Option<GraphSnapshotId>,
    pub store_path: PathBuf,
    pub diagnostics: Vec<GraphCliDiagnostic>,
}

/// Result of graph verify.
#[derive(Serialize)]
pub struct GraphVerifyResult {
    pub status: String,
    pub head: Option<GraphSnapshotId>,
    pub diagnostics: Vec<GraphCliDiagnostic>,
}

/// Result of graph clean.
#[derive(Serialize)]
pub struct GraphCleanResult {
    pub status: String,
    pub removed_snapshots: usize,
    pub diagnostics: Vec<GraphCliDiagnostic>,
}

#[derive(Serialize)]
pub struct GraphCliDiagnostic {
    pub level: String,
    pub message: String,
}

impl From<&tree_sitter_context::schema::Diagnostic> for GraphCliDiagnostic {
    fn from(d: &tree_sitter_context::schema::Diagnostic) -> Self {
        Self {
            level: format!("{:?}", d.level).to_lowercase(),
            message: d.message.clone(),
        }
    }
}

/// Build a graph snapshot from the repo.
pub fn build_graph(opts: &GraphBuildOptions) -> Result<GraphBuildResult> {
    let loader = build_loader(opts.grammar_path.as_deref())?;
    let store = GraphStore::open(&opts.repo_root)
        .with_context(|| format!("failed to open graph store at {}", opts.repo_root.display()))?;

    let mut graph_files = Vec::new();
    let mut diagnostics: Vec<GraphCliDiagnostic> = Vec::new();
    let mut files_scanned = 0;
    let mut files_included = 0;

    for entry in WalkDir::new(&opts.repo_root)
        .into_iter()
        .filter_entry(|e| !is_ignored_path(e.path(), &opts.repo_root))
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        files_scanned += 1;

        let repo_relative = match path.strip_prefix(&opts.repo_root) {
            Ok(p) => p,
            Err(_) => {
                diagnostics.push(GraphCliDiagnostic {
                    level: "warning".to_string(),
                    message: format!("skipping path outside repo root: {}", path.display()),
                });
                continue;
            }
        };

        let source = match fs::read(path) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(GraphCliDiagnostic {
                    level: "warning".to_string(),
                    message: format!("skipping unreadable file {}: {e}", path.display()),
                });
                continue;
            }
        };

        let (language, language_config) = match loader.language_configuration_for_file_name(path)? {
            Some(lc) => lc,
            None => {
                diagnostics.push(GraphCliDiagnostic {
                    level: "info".to_string(),
                    message: format!("no language for {}, skipped", path.display()),
                });
                continue;
            }
        };

        let mut parser = Parser::new();
        parser.set_language(&language)?;

        let tree = match parser.parse(&source, None) {
            Some(t) => t,
            None => {
                diagnostics.push(GraphCliDiagnostic {
                    level: "warning".to_string(),
                    message: format!("parse failed for {}, skipped", path.display()),
                });
                continue;
            }
        };

        let chunk_output = chunks_for_tree(
            &tree,
            repo_relative,
            &source,
            &ChunkOptions::default(),
        );

        let tags_config = language_config.tags_config(language)?;
        let symbol_output = tags_config.map(|tc| {
            symbols_for_tree(repo_relative, &source, tc, &SymbolOptions::default())
        });

        let file_content_hash = Some(format!("{:032x}", xxhash_rust::xxh3::xxh3_128(&source)));

        let graph_file = extract_graph_file(
            repo_relative,
            &source,
            &chunk_output,
            symbol_output.as_ref(),
            file_content_hash,
            tags_config.is_none(),
        );

        for d in &graph_file.diagnostics {
            diagnostics.push(d.into());
        }

        graph_files.push(graph_file);
        files_included += 1;
    }

    let total_nodes: usize = graph_files.iter().map(|f| f.nodes.len()).sum();

    let snapshot = canonicalize_snapshot(GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: graph_files,
        edges: vec![], // U6 will populate this
        diagnostics: vec![],
            meta: Some(GraphMeta {
                created_at: Some(format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())),
                repo_root: Some(opts.repo_root.clone()),
                total_files: files_included,
                total_nodes,
                total_edges: 0,
            }),
    });

    let snapshot_id = store
        .write_snapshot(snapshot)
        .map_err(|e| anyhow!("failed to write snapshot: {e}"))?;

    store
        .update_head(&snapshot_id)
        .map_err(|e| anyhow!("failed to update HEAD: {e}"))?;

    Ok(GraphBuildResult {
        status: "ok".to_string(),
        snapshot_id,
        files_scanned,
        files_included,
        total_nodes,
        total_edges: 0,
        diagnostics,
    })
}

/// Get graph status.
pub fn graph_status(repo_root: &Path) -> Result<GraphStatusResult> {
    let store = GraphStore::open(repo_root)?;
    let head = store.read_head().ok();

    let mut diagnostics = Vec::new();
    if head.is_none() {
        diagnostics.push(GraphCliDiagnostic {
            level: "info".to_string(),
            message: "no HEAD snapshot found".to_string(),
        });
    }

    Ok(GraphStatusResult {
        status: "ok".to_string(),
        head,
        store_path: store.path().to_path_buf(),
        diagnostics,
    })
}

/// Verify graph store integrity.
pub fn graph_verify(repo_root: &Path) -> Result<GraphVerifyResult> {
    let store = GraphStore::open(repo_root)?;
    let head = store.read_head().ok();

    let mut diagnostics = Vec::new();

    match store.verify() {
        Ok(()) => {
            diagnostics.push(GraphCliDiagnostic {
                level: "info".to_string(),
                message: "store verification passed".to_string(),
            });
        }
        Err(e) => {
            diagnostics.push(GraphCliDiagnostic {
                level: "error".to_string(),
                message: format!("store verification failed: {e}"),
            });
            return Ok(GraphVerifyResult {
                status: "error".to_string(),
                head,
                diagnostics,
            });
        }
    }

    Ok(GraphVerifyResult {
        status: "ok".to_string(),
        head,
        diagnostics,
    })
}

/// Clean unreachable snapshots.
/// Update graph incrementally from previous HEAD.
pub fn graph_update(opts: &GraphBuildOptions) -> Result<GraphBuildResult> {
    let loader = build_loader(opts.grammar_path.as_deref())?;
    let store = GraphStore::open(&opts.repo_root)
        .with_context(|| format!("failed to open graph store at {}", opts.repo_root.display()))?;

    // Load previous HEAD snapshot if available
    let previous = store.read_head().ok().and_then(|id| store.read_snapshot(&id).ok());
    let previous_files: std::collections::HashMap<PathBuf, tree_sitter_context::GraphFile> =
        previous
            .as_ref()
            .map(|s| s.files.iter().map(|f| (f.path.clone(), f.clone())).collect())
            .unwrap_or_default();

    let mut graph_files = Vec::new();
    let mut diagnostics: Vec<GraphCliDiagnostic> = Vec::new();
    let mut files_scanned = 0;
    let mut files_included = 0;
    let mut files_unchanged = 0;

    let mut current_paths = std::collections::HashSet::new();

    for entry in WalkDir::new(&opts.repo_root)
        .into_iter()
        .filter_entry(|e| !is_ignored_path(e.path(), &opts.repo_root))
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        files_scanned += 1;

        let repo_relative = match path.strip_prefix(&opts.repo_root) {
            Ok(p) => p,
            Err(_) => {
                diagnostics.push(GraphCliDiagnostic {
                    level: "warning".to_string(),
                    message: format!("skipping path outside repo root: {}", path.display()),
                });
                continue;
            }
        };

        current_paths.insert(repo_relative.to_path_buf());

        let source = match fs::read(path) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(GraphCliDiagnostic {
                    level: "warning".to_string(),
                    message: format!("skipping unreadable file {}: {e}", path.display()),
                });
                continue;
            }
        };

        let file_content_hash = Some(format!("{:032x}", xxhash_rust::xxh3::xxh3_128(&source)));

        // Check if unchanged from previous snapshot
        if let Some(prev_file) = previous_files.get(repo_relative) {
            if prev_file.content_hash == file_content_hash {
                files_unchanged += 1;
                graph_files.push(prev_file.clone());
                continue;
            }
        }

        let (language, language_config) = match loader.language_configuration_for_file_name(path)? {
            Some(lc) => lc,
            None => {
                diagnostics.push(GraphCliDiagnostic {
                    level: "info".to_string(),
                    message: format!("no language for {}, skipped", path.display()),
                });
                continue;
            }
        };

        let mut parser = Parser::new();
        parser.set_language(&language)?;

        let tree = match parser.parse(&source, None) {
            Some(t) => t,
            None => {
                diagnostics.push(GraphCliDiagnostic {
                    level: "warning".to_string(),
                    message: format!("parse failed for {}, skipped", path.display()),
                });
                continue;
            }
        };

        let chunk_output = chunks_for_tree(
            &tree,
            repo_relative,
            &source,
            &ChunkOptions::default(),
        );

        let tags_config = language_config.tags_config(language)?;
        let symbol_output = tags_config.map(|tc| {
            symbols_for_tree(repo_relative, &source, tc, &SymbolOptions::default())
        });

        let graph_file = extract_graph_file(
            repo_relative,
            &source,
            &chunk_output,
            symbol_output.as_ref(),
            file_content_hash,
            tags_config.is_none(),
        );

        for d in &graph_file.diagnostics {
            diagnostics.push(d.into());
        }

        graph_files.push(graph_file);
        files_included += 1;
    }

    // Mark deleted files
    for (prev_path, prev_file) in &previous_files {
        if !current_paths.contains(prev_path) {
            let mut deleted_file = prev_file.clone();
            deleted_file.content_hash = None;
            deleted_file.diagnostics.push(tree_sitter_context::schema::Diagnostic::warn(
                "file deleted since last snapshot",
            ));
            graph_files.push(deleted_file);
        }
    }

    let total_nodes: usize = graph_files.iter().map(|f| f.nodes.len()).sum();

    let snapshot = canonicalize_snapshot(GraphSnapshot {
        schema_version: GRAPH_SCHEMA_VERSION.to_string(),
        snapshot_id: GraphSnapshotId(String::new()),
        files: graph_files,
        edges: vec![], // U6 will populate this
        diagnostics: vec![],
        meta: Some(GraphMeta {
            created_at: Some(format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())),
            repo_root: Some(opts.repo_root.clone()),
            total_files: files_included + files_unchanged,
            total_nodes,
            total_edges: 0,
        }),
    });

    let snapshot_id = store
        .write_snapshot(snapshot)
        .map_err(|e| anyhow!("failed to write snapshot: {e}"))?;

    store
        .update_head(&snapshot_id)
        .map_err(|e| anyhow!("failed to update HEAD: {e}"))?;

    Ok(GraphBuildResult {
        status: "ok".to_string(),
        snapshot_id,
        files_scanned,
        files_included: files_included + files_unchanged,
        total_nodes,
        total_edges: 0,
        diagnostics,
    })
}

/// Diff two snapshots.
pub fn graph_diff(
    repo_root: &Path,
    from_snapshot_id: &GraphSnapshotId,
    to_snapshot_id: &GraphSnapshotId,
) -> Result<tree_sitter_context::graph::diff::GraphDiff> {
    let store = GraphStore::open(repo_root)?;
    let from = store
        .read_snapshot(from_snapshot_id)
        .map_err(|e| anyhow!("failed to read from snapshot: {e}"))?;
    let to = store
        .read_snapshot(to_snapshot_id)
        .map_err(|e| anyhow!("failed to read to snapshot: {e}"))?;

    tree_sitter_context::graph::diff::diff_snapshots(&from, &to)
        .map_err(|e| anyhow!("diff failed: {e}"))
}

pub fn graph_clean(repo_root: &Path) -> Result<GraphCleanResult> {
    let store = GraphStore::open(repo_root)?;
    let removed = store
        .clean()
        .map_err(|e| anyhow!("clean failed: {e}"))?;

    Ok(GraphCleanResult {
        status: "ok".to_string(),
        removed_snapshots: removed,
        diagnostics: vec![GraphCliDiagnostic {
            level: "info".to_string(),
            message: format!("removed {removed} unreachable snapshots"),
        }],
    })
}

fn build_loader(grammar_path: Option<&Path>) -> Result<Loader> {
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

fn is_ignored_path(path: &Path, repo_root: &Path) -> bool {
    let relative = path.strip_prefix(repo_root).unwrap_or(path);
    let components: Vec<_> = relative.components().collect();

    // Skip common generated directories and VCS
    for component in &components {
        let name = component.as_os_str().to_string_lossy();
        if name.starts_with('.')
            && matches!(
                name.as_ref(),
                ".git"
                    | ".svn"
                    | ".hg"
                    | ".tree-sitter-context-mcp"
                    | ".target"
                    | ".node_modules"
                    | ".venv"
                    | ".tox"
            )
        {
            return false;
        }
        if matches!(name.as_ref(), "target" | "node_modules" | "vendor" | "dist" | "build") {
            return false;
        }
    }
    true
}

pub fn render_json(value: &impl Serialize) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}

/// Options for orientation get.
pub struct OrientationGetOptions {
    pub repo_root: PathBuf,
    pub budget: Option<usize>,
    pub format: OrientationFormat,
}

/// Output format for orientation.
pub enum OrientationFormat {
    Sexpr,
    Json,
}

/// Result of orientation get.
pub struct OrientationGetResult {
    pub bytes: Vec<u8>,
    pub format: OrientationFormat,
}

/// Get the current orientation block for the repo.
pub fn orientation_get(opts: &OrientationGetOptions) -> Result<OrientationGetResult> {
    let store = GraphStore::open(&opts.repo_root)
        .with_context(|| format!("failed to open graph store at {}", opts.repo_root.display()))?;

    let head_id = store.read_head().map_err(|e| match e {
        tree_sitter_context::GraphError::MissingSnapshot { .. } => {
            anyhow!("no_graph: run `tree-sitter-context graph build` first")
        }
        tree_sitter_context::GraphError::CorruptedSnapshot { reason, .. } => {
            anyhow!("graph_corrupt: {}", reason)
        }
        tree_sitter_context::GraphError::SchemaMismatch { expected, found } => {
            anyhow!("schema_mismatch: expected={}, found={}", expected, found)
        }
        e => anyhow!("graph_corrupt: {}", e),
    })?;

    let snapshot = store
        .read_snapshot(&head_id)
        .map_err(|e| anyhow!("graph_corrupt: failed to read snapshot: {}", e))?;

    let block = tree_sitter_context::build_orientation(&snapshot, opts.budget);

    let bytes = match opts.format {
        OrientationFormat::Sexpr => {
            tree_sitter_context::sexpr::orientation_to_sexpr(&block)
                .map_err(|e| anyhow!("failed to serialize orientation: {}", e))?
        }
        OrientationFormat::Json => {
            let json = serde_json::to_string_pretty(&block)
                .map_err(|e| anyhow!("failed to serialize orientation: {}", e))?;
            format!("{}\n", json).into_bytes()
        }
    };

    Ok(OrientationGetResult {
        bytes,
        format: OrientationFormat::Sexpr,
    })
}

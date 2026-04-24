//! Invalidation logic for context chunks.
//!
//! Milestone 3: Accept old/new file pair OR `InputEdit` sequence,
//! map changed ranges to affected chunks, and produce an invalidation
//! result with confidence levels.

use std::path::Path;

use tree_sitter::{InputEdit, Tree};

use crate::schema::{ChunkId, ChunkRecord, ContextOutput, Diagnostic};

/// Result of comparing two snapshots of the same file.
#[derive(Clone, Debug)]
pub struct InvalidationResult {
    pub affected: Vec<ChunkId>,
    pub unchanged: Vec<ChunkId>,
    pub new: Vec<ChunkRecord>,
    pub removed: Vec<ChunkId>,
}

/// Compare an old and new snapshot of a file.
///
/// This is a placeholder implementation for Milestone 3.
/// It currently returns every chunk as affected so that callers
/// can build against the API shape.
pub fn invalidate_snapshot(
    _old_tree: &Tree,
    new_tree: &Tree,
    source: &[u8],
    path: &Path,
) -> ContextOutput {
    use crate::chunk::{ChunkOptions, chunks_for_tree};

    let options = ChunkOptions::default();
    let chunks = chunks_for_tree(new_tree, path, source, &options);

    let mut output = ContextOutput::new("0.1.0").with_source_path(path);

    output.push_diagnostic(Diagnostic::warn(
        "invalidate_snapshot is a Milestone 3 placeholder; all chunks treated as affected",
    ));

    for chunk in chunks {
        output.push_chunk(chunk);
    }

    output
}

/// Apply a sequence of edits and return affected chunks.
///
/// This is a placeholder implementation for Milestone 3.
pub fn invalidate_edits(
    _tree: &Tree,
    source: &[u8],
    _edits: &[InputEdit],
    path: &Path,
) -> ContextOutput {
    use crate::chunk::{ChunkOptions, chunks_for_tree};

    let options = ChunkOptions::default();
    let chunks = chunks_for_tree(_tree, path, source, &options);

    let mut output = ContextOutput::new("0.1.0").with_source_path(path);

    output.push_diagnostic(Diagnostic::warn(
        "invalidate_edits is a Milestone 3 placeholder; all chunks treated as affected",
    ));

    for chunk in chunks {
        output.push_chunk(chunk);
    }

    output
}

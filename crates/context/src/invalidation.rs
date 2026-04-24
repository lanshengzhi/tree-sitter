use std::path::Path;

use anyhow::{Result, anyhow};
use tree_sitter::{InputEdit, Parser, Tree};

use crate::{
    chunk::{ChunkOptions, chunks_for_tree},
    identity::{MatchResult, match_chunks},
    schema::{ByteRange, Confidence, Diagnostic, InvalidationOutput, OutputMeta},
};

/// Compare an old and new snapshot of a file.
///
/// Uses tree-sitter's `changed_ranges` to detect affected regions and
/// stable identity matching to classify added/removed/unchanged chunks.
pub fn invalidate_snapshot(
    old_tree: &Tree,
    new_tree: &Tree,
    old_source: &[u8],
    new_source: &[u8],
    path: &Path,
) -> Result<InvalidationOutput> {
    let options = ChunkOptions::default();

    let old_chunks = chunks_for_tree(old_tree, path, old_source, &options);
    let new_chunks = chunks_for_tree(new_tree, path, new_source, &options);

    let changed_ranges: Vec<ByteRange> = old_tree
        .changed_ranges(new_tree)
        .map(|r| ByteRange::from(r.start_byte..r.end_byte))
        .collect();

    let mut output = InvalidationOutput {
        affected: Vec::new(),
        added: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        changed_ranges: changed_ranges.clone(),
        diagnostics: Vec::new(),
        meta: OutputMeta {
            schema_version: "0.1.0".to_string(),
            source_path: Some(path.to_path_buf()),
            total_chunks: new_chunks.len(),
            total_estimated_tokens: new_chunks.iter().map(|c| c.estimated_tokens).sum(),
        },
    };

    if changed_ranges.is_empty() {
        output.diagnostics.push(Diagnostic::info(
            "no changed ranges detected between snapshots",
        ));
    }

    // Classify chunks by stable identity.
    let matches = match_chunks(&old_chunks, &new_chunks);
    for m in matches {
        match m {
            MatchResult::Unchanged { old: _, new } => {
                if overlaps_any(&new.byte_range, &changed_ranges) {
                    output.affected.push(new);
                } else {
                    output.unchanged.push(new);
                }
            }
            MatchResult::Removed { old } => {
                output.removed.push(old);
            }
            MatchResult::Added { new } => {
                output.added.push(new);
            }
        }
    }

    // Any new chunk that overlaps a changed range but was not caught by
    // identity matching (e.g. a completely new unnamed block) is also affected.
    for chunk in &new_chunks {
        if !output
            .affected
            .iter()
            .any(|c| c.stable_id == chunk.stable_id)
            && !output.added.iter().any(|c| c.stable_id == chunk.stable_id)
            && !output
                .unchanged
                .iter()
                .any(|c| c.stable_id == chunk.stable_id)
            && overlaps_any(&chunk.byte_range, &changed_ranges)
        {
            output.affected.push(chunk.clone());
        }
    }

    if !output.affected.is_empty() {
        output.diagnostics.push(Diagnostic::info(format!(
            "{} affected chunk(s)",
            output.affected.len()
        )));
    }
    if !output.added.is_empty() {
        output.diagnostics.push(Diagnostic::info(format!(
            "{} added chunk(s)",
            output.added.len()
        )));
    }
    if !output.removed.is_empty() {
        output.diagnostics.push(Diagnostic::info(format!(
            "{} removed chunk(s)",
            output.removed.len()
        )));
    }

    Ok(output)
}

/// Apply a sequence of edits and return affected chunks.
///
/// This uses incremental re-parsing. Confidence is Medium rather than Exact
/// because incremental parse correctness depends on edit boundary alignment.
pub fn invalidate_edits(
    parser: &mut Parser,
    old_tree: &Tree,
    source: &[u8],
    new_source: &[u8],
    edits: &[InputEdit],
    path: &Path,
) -> Result<InvalidationOutput> {
    let mut tree = old_tree.clone();
    for edit in edits {
        tree.edit(edit);
    }

    let new_tree = parser
        .parse(new_source, Some(&tree))
        .ok_or_else(|| anyhow!("incremental re-parse failed for {}", path.display()))?;

    let mut output = invalidate_snapshot(&tree, &new_tree, source, new_source, path)?;

    // Downgrade confidence for edit-stream invalidation.
    for chunk in &mut output.affected {
        chunk.confidence = Confidence::Medium;
    }
    for chunk in &mut output.added {
        chunk.confidence = Confidence::Medium;
    }

    output.diagnostics.push(Diagnostic::info(
        "edit-stream invalidation: confidence downgraded to Medium because incremental parse may miss some changes",
    ));

    Ok(output)
}

fn overlaps_any(range: &ByteRange, ranges: &[ByteRange]) -> bool {
    ranges
        .iter()
        .any(|r| range.start < r.end && range.end > r.start)
}

// Unit tests for invalidation require a compiled tree-sitter grammar
// (e.g. tree-sitter-rust) which is not available in the standard test
// environment. Integration tests should be added under crates/cli/tests/
// or tested manually with:
//
//   cargo run -- context --old old.rs new.rs
//
// Required test coverage:
// 1. body_only_change: edit function body, expect 1 affected chunk.
// 2. signature_change: edit function signature, expect 1 affected chunk.
// 3. doc_only_change: edit doc comment, expect 1 affected chunk.
// 4. whitespace_only_change: reformat whitespace, expect 0 affected chunks.
// 5. edit_sequence_correctness: apply multiple InputEdits, verify affected set.

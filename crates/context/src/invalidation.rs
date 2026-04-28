use std::path::Path;

use anyhow::{Result, anyhow};
use similar::{DiffTag, TextDiff};
use tree_sitter::{InputEdit, Parser, Tree};

use crate::{
    chunk::{ChunkOptions, chunks_for_tree},
    identity::{MatchResult, match_chunks},
    schema::{
        ByteRange, ChangeType, Confidence, Diagnostic, InvalidationOutput, InvalidationReason,
        InvalidationRecord, InvalidationStatus, MatchStrategy, OutputMeta,
    },
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

    let old_result = chunks_for_tree(old_tree, path, old_source, &options);
    let new_result = chunks_for_tree(new_tree, path, new_source, &options);
    let old_chunks = old_result.chunks;
    let new_chunks = new_result.chunks;

    let changed_ranges = snapshot_changed_ranges(old_source, new_source);

    let mut output = InvalidationOutput {
        records: Vec::new(),
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

    output.diagnostics.extend(
        old_result
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.with_source("old_snapshot_chunking")),
    );
    output.diagnostics.extend(
        new_result
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.with_source("new_snapshot_chunking")),
    );

    // Classify chunks by stable identity.
    let matches = match_chunks(&old_chunks, &new_chunks);
    for m in matches {
        match m {
            MatchResult::Unchanged { old, new } => {
                if !source_equal_ignoring_whitespace(
                    old_source,
                    &old.byte_range,
                    new_source,
                    &new.byte_range,
                ) {
                    // Content changed even if changed_ranges (from independent
                    // parses) did not report it — e.g. literal-only changes.
                    if !overlaps_any(&new.byte_range, &changed_ranges) {
                        output.changed_ranges.push(new.byte_range);
                    }
                    let change_type = classify_change(&old, &new);
                    output.records.push(InvalidationRecord {
                        status: InvalidationStatus::Affected,
                        chunk: new.clone(),
                        old_chunk: Some(old),
                        reason: InvalidationReason::ContentChanged,
                        match_strategy: MatchStrategy::StableId,
                        confidence: new.confidence,
                        changed_ranges: ranges_overlapping_or_exact(
                            &new.byte_range,
                            &output.changed_ranges,
                        ),
                        change_type,
                    });
                    output.affected.push(new);
                } else {
                    output.records.push(InvalidationRecord {
                        status: InvalidationStatus::Unchanged,
                        chunk: new.clone(),
                        old_chunk: Some(old),
                        reason: InvalidationReason::NoChangeDetected,
                        match_strategy: MatchStrategy::StableId,
                        confidence: new.confidence,
                        changed_ranges: Vec::new(),
                        change_type: ChangeType::Unchanged,
                    });
                    output.unchanged.push(new);
                }
            }
            MatchResult::Removed { old } => {
                output.records.push(InvalidationRecord {
                    status: InvalidationStatus::Removed,
                    chunk: old.clone(),
                    old_chunk: Some(old.clone()),
                    reason: InvalidationReason::RemovedChunk,
                    match_strategy: MatchStrategy::Unmatched,
                    confidence: old.confidence,
                    changed_ranges: Vec::new(),
                    change_type: ChangeType::Removed,
                });
                output.removed.push(old);
            }
            MatchResult::Added { new } => {
                output.records.push(InvalidationRecord {
                    status: InvalidationStatus::Added,
                    chunk: new.clone(),
                    old_chunk: None,
                    reason: InvalidationReason::AddedChunk,
                    match_strategy: MatchStrategy::Unmatched,
                    confidence: new.confidence,
                    changed_ranges: ranges_overlapping_or_exact(&new.byte_range, &changed_ranges),
                    change_type: ChangeType::Added,
                });
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
            output.records.push(InvalidationRecord {
                status: InvalidationStatus::Affected,
                chunk: chunk.clone(),
                old_chunk: None,
                reason: InvalidationReason::ChangedRangeOverlap,
                match_strategy: MatchStrategy::TextualRangeOverlap,
                confidence: chunk.confidence,
                changed_ranges: ranges_overlapping_or_exact(&chunk.byte_range, &changed_ranges),
                change_type: ChangeType::BothChanged,
            });
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

    // Use the original old_tree for chunk extraction so byte ranges align
    // with the old source. The edited tree is only needed for incremental parsing.
    let mut output = invalidate_snapshot(old_tree, &new_tree, source, new_source, path)?;

    // Downgrade confidence for edit-stream invalidation.
    for chunk in &mut output.affected {
        chunk.confidence = Confidence::Medium;
    }
    for chunk in &mut output.added {
        chunk.confidence = Confidence::Medium;
    }
    for chunk in &mut output.removed {
        chunk.confidence = Confidence::Medium;
    }
    for record in &mut output.records {
        record.confidence = Confidence::Medium;
        record.chunk.confidence = Confidence::Medium;
        if let Some(old_chunk) = &mut record.old_chunk {
            old_chunk.confidence = Confidence::Medium;
        }
        if record.match_strategy == MatchStrategy::TextualRangeOverlap {
            record.match_strategy = MatchStrategy::EditRangeOverlap;
        }
    }

    output.diagnostics.push(Diagnostic::info(
        "edit-stream invalidation: confidence downgraded to Medium because incremental parse may miss some changes",
    ));

    Ok(output)
}

/// Classify the type of change between two matched chunks.
fn classify_change(old: &crate::schema::ChunkRecord, new: &crate::schema::ChunkRecord) -> ChangeType {
    let sig_changed = old.signature_hash != new.signature_hash;
    let body_changed = old.body_hash != new.body_hash;

    match (sig_changed, body_changed) {
        (true, true) => ChangeType::BothChanged,
        (true, false) => ChangeType::SignatureChanged,
        (false, true) => ChangeType::BodyChanged,
        (false, false) => ChangeType::Unchanged,
    }
}

fn overlaps_any(range: &ByteRange, ranges: &[ByteRange]) -> bool {
    ranges
        .iter()
        .any(|r| range.start < r.end && range.end > r.start)
}

fn snapshot_changed_ranges(old_source: &[u8], new_source: &[u8]) -> Vec<ByteRange> {
    if old_source == new_source {
        return Vec::new();
    }

    let (Ok(old_text), Ok(new_text)) = (
        std::str::from_utf8(old_source),
        std::str::from_utf8(new_source),
    ) else {
        return vec![ByteRange {
            start: 0,
            end: new_source.len(),
        }];
    };

    let line_offsets = line_start_offsets(new_text);
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut ranges = Vec::new();

    for op in diff.ops() {
        if op.tag() == DiffTag::Equal {
            continue;
        }

        let new_range = op.new_range();
        let start = line_offsets
            .get(new_range.start)
            .copied()
            .unwrap_or(new_source.len());
        let end = line_offsets
            .get(new_range.end)
            .copied()
            .unwrap_or(new_source.len());

        ranges.push(ByteRange { start, end });
    }

    ranges
}

fn line_start_offsets(text: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            offsets.push(index + 1);
        }
    }
    offsets.push(text.len());
    offsets
}

fn ranges_overlapping_or_exact(range: &ByteRange, ranges: &[ByteRange]) -> Vec<ByteRange> {
    let overlaps: Vec<_> = ranges
        .iter()
        .copied()
        .filter(|changed| range.start < changed.end && range.end > changed.start)
        .collect();

    if overlaps.is_empty() {
        vec![*range]
    } else {
        overlaps
    }
}

/// Compare two source slices (within given byte ranges) for equality,
/// ignoring ASCII whitespace.
///
/// This prevents whitespace-only reformats from being classified as affected.
fn source_equal_ignoring_whitespace(
    old_source: &[u8],
    old_range: &ByteRange,
    new_source: &[u8],
    new_range: &ByteRange,
) -> bool {
    let old = &old_source[old_range.start..old_range.end.min(old_source.len())];
    let new = &new_source[new_range.start..new_range.end.min(new_source.len())];
    old.iter()
        .filter(|&&c| !c.is_ascii_whitespace())
        .eq(new.iter().filter(|&&c| !c.is_ascii_whitespace()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tree_sitter::{InputEdit, Parser, Point};

    fn rust_parser() -> Parser {
        let raw = unsafe { tree_sitter_rust::LANGUAGE.into_raw()() };
        let language: tree_sitter::Language = unsafe { std::mem::transmute(raw) };
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        parser
    }

    #[test]
    fn body_only_change() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() { let x = 1; }";
        let new_source = b"fn foo() { let x = 1; let y = 2; }";

        let old_tree = parser.parse(old_source, None).unwrap();
        let new_tree = parser.parse(new_source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            old_source,
            new_source,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(
            output.affected.len(),
            1,
            "expected 1 affected chunk, got affected={:?} added={:?} removed={:?} unchanged={:?}",
            output.affected,
            output.added,
            output.removed,
            output.unchanged
        );
        assert_eq!(output.records.len(), 1);
        assert_eq!(output.records[0].status, InvalidationStatus::Affected);
        assert_eq!(output.records[0].reason, InvalidationReason::ContentChanged);
        assert_eq!(output.records[0].match_strategy, MatchStrategy::StableId);
        assert!(output.added.is_empty());
        assert!(output.removed.is_empty());
    }

    #[test]
    fn signature_change() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() {}";
        let new_source = b"fn foo(x: i32) {}";

        let old_tree = parser.parse(old_source, None).unwrap();
        let new_tree = parser.parse(new_source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            old_source,
            new_source,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(
            output.affected.len(),
            1,
            "expected 1 affected chunk, got affected={:?} added={:?} removed={:?} unchanged={:?}",
            output.affected,
            output.added,
            output.removed,
            output.unchanged
        );
        assert!(output.added.is_empty());
        assert!(output.removed.is_empty());
    }

    #[test]
    fn comment_inside_body_change() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() { let x = 1; }";
        let new_source = b"fn foo() { let x = 1; /* note */ }";

        let old_tree = parser.parse(old_source, None).unwrap();
        let new_tree = parser.parse(new_source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            old_source,
            new_source,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(
            output.affected.len(),
            1,
            "expected 1 affected chunk, got affected={:?} added={:?} removed={:?} unchanged={:?}",
            output.affected,
            output.added,
            output.removed,
            output.unchanged
        );
    }

    #[test]
    fn whitespace_only_change() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() { let x = 1; }";
        let new_source = b"fn foo() {\n    let x = 1;\n}";

        let old_tree = parser.parse(old_source, None).unwrap();
        let new_tree = parser.parse(new_source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            old_source,
            new_source,
            Path::new("test.rs"),
        )
        .unwrap();

        assert!(
            output.affected.is_empty(),
            "expected 0 affected chunks for whitespace-only change, got affected={:?} added={:?} removed={:?}",
            output.affected,
            output.added,
            output.removed
        );
    }

    #[test]
    fn edit_sequence_correctness() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() { let x = 1; }";
        let new_source = b"fn bar() { let x = 2; }";

        let old_tree = parser.parse(old_source, None).unwrap();

        // Two edits: rename foo -> bar, and change 1 -> 2.
        let edits = vec![
            InputEdit {
                start_byte: 3,
                old_end_byte: 6,
                new_end_byte: 6,
                start_position: Point::new(0, 3),
                old_end_position: Point::new(0, 6),
                new_end_position: Point::new(0, 6),
            },
            InputEdit {
                start_byte: 19,
                old_end_byte: 20,
                new_end_byte: 20,
                start_position: Point::new(0, 19),
                old_end_position: Point::new(0, 20),
                new_end_position: Point::new(0, 20),
            },
        ];

        let output = invalidate_edits(
            &mut parser,
            &old_tree,
            old_source,
            new_source,
            &edits,
            Path::new("test.rs"),
        )
        .unwrap();

        // Name change -> old removed, new added. Body change overlaps changed range
        // but the new chunk is already in added, so it should not be duplicated in affected.
        assert_eq!(
            output.added.len(),
            1,
            "expected 1 added chunk, got added={:?} affected={:?} removed={:?}",
            output.added,
            output.affected,
            output.removed
        );
        assert_eq!(output.removed.len(), 1, "expected 1 removed chunk");
        assert_eq!(output.added[0].confidence, Confidence::Medium);
        assert_eq!(output.removed[0].confidence, Confidence::Medium);
        assert!(
            output
                .records
                .iter()
                .all(|record| record.confidence == Confidence::Medium)
        );
        assert!(
            output.affected.is_empty(),
            "expected 0 affected chunks when name changes, got affected={:?}",
            output.affected
        );
    }

    #[test]
    fn edit_length_change() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() { let x = 1; }";
        let new_source = b"fn foo() { let x = 10; }";

        let old_tree = parser.parse(old_source, None).unwrap();

        let edits = vec![InputEdit {
            start_byte: 19,
            old_end_byte: 20,
            new_end_byte: 21,
            start_position: Point::new(0, 19),
            old_end_position: Point::new(0, 20),
            new_end_position: Point::new(0, 21),
        }];

        let output = invalidate_edits(
            &mut parser,
            &old_tree,
            old_source,
            new_source,
            &edits,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(
            output.affected.len(),
            1,
            "expected 1 affected chunk for length-changing body edit, got affected={:?} added={:?} removed={:?}",
            output.affected,
            output.added,
            output.removed
        );
        assert!(output.added.is_empty());
        assert!(output.removed.is_empty());
    }

    #[test]
    fn literal_only_change() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() { let x = \"foo\"; }";
        let new_source = b"fn foo() { let x = \"bar\"; }";

        let old_tree = parser.parse(old_source, None).unwrap();
        let new_tree = parser.parse(new_source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            old_source,
            new_source,
            Path::new("test.rs"),
        )
        .unwrap();

        assert!(
            !output.affected.is_empty(),
            "expected at least 1 affected chunk for literal-only change, got affected={:?} added={:?} removed={:?} unchanged={:?}",
            output.affected,
            output.added,
            output.removed,
            output.unchanged
        );
        assert!(
            !output.changed_ranges.is_empty(),
            "expected textual snapshot ranges for literal-only change"
        );
    }

    #[test]
    fn body_only_change_produces_body_changed() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() { let x = 1; }";
        let new_source = b"fn foo() { let x = 1; let y = 2; }";

        let old_tree = parser.parse(old_source, None).unwrap();
        let new_tree = parser.parse(new_source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            old_source,
            new_source,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(output.records.len(), 1);
        assert_eq!(output.records[0].change_type, ChangeType::BodyChanged);
    }

    #[test]
    fn signature_change_produces_signature_changed() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() {}";
        let new_source = b"fn foo(x: i32) {}";

        let old_tree = parser.parse(old_source, None).unwrap();
        let new_tree = parser.parse(new_source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            old_source,
            new_source,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(output.records.len(), 1);
        assert_eq!(output.records[0].change_type, ChangeType::SignatureChanged);
    }

    #[test]
    fn both_changed_produces_both_changed() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() {}";
        let new_source = b"fn bar(x: i32) { let y = 1; }";

        let old_tree = parser.parse(old_source, None).unwrap();
        let new_tree = parser.parse(new_source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            old_source,
            new_source,
            Path::new("test.rs"),
        )
        .unwrap();

        // Name change causes old removed + new added, not matched
        // So we test a case where stable_id matches but both hashes differ
        let old_source2 = b"fn foo() { let x = 1; }";
        let new_source2 = b"fn foo(x: i32) { let y = 2; }";

        let old_tree2 = parser.parse(old_source2, None).unwrap();
        let new_tree2 = parser.parse(new_source2, None).unwrap();

        let output2 = invalidate_snapshot(
            &old_tree2,
            &new_tree2,
            old_source2,
            new_source2,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(output2.records.len(), 1);
        assert_eq!(output2.records[0].change_type, ChangeType::BothChanged);
    }

    #[test]
    fn unchanged_file_produces_unchanged() {
        let mut parser = rust_parser();
        let source = b"fn foo() { let x = 1; }";

        let old_tree = parser.parse(source, None).unwrap();
        let new_tree = parser.parse(source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            source,
            source,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(output.records.len(), 1);
        assert_eq!(output.records[0].change_type, ChangeType::Unchanged);
    }

    #[test]
    fn added_removed_functions_produce_correct_change_types() {
        let mut parser = rust_parser();
        let old_source = b"fn foo() {}";
        let new_source = b"fn bar() {}";

        let old_tree = parser.parse(old_source, None).unwrap();
        let new_tree = parser.parse(new_source, None).unwrap();

        let output = invalidate_snapshot(
            &old_tree,
            &new_tree,
            old_source,
            new_source,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(output.records.len(), 2);
        let removed = output.records.iter().find(|r| r.status == InvalidationStatus::Removed).unwrap();
        let added = output.records.iter().find(|r| r.status == InvalidationStatus::Added).unwrap();
        assert_eq!(removed.change_type, ChangeType::Removed);
        assert_eq!(added.change_type, ChangeType::Added);
    }
}

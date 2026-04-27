//! Canonical S-expression serializer for the R0 v1 protocol.
//!
//! Produces deterministic, byte-stable output for all bundle result types.

use std::io::{self, Write};

use crate::protocol::{
    AmbiguousStableId, AstCell, Bundle, BundleResult, Candidate, Exhausted, NotFound,
    OmittedChunk, Provenance, UnknownCrossFile,
};

/// Serialize a bundle result to canonical S-expression bytes.
pub fn serialize(result: &BundleResult) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    serialize_result(&mut buf, result, 0)?;
    buf.push(b'\n');
    Ok(buf)
}

fn serialize_result(w: &mut impl Write, result: &BundleResult, depth: usize) -> io::Result<()> {
    match result {
        BundleResult::Bundle(b) => serialize_bundle(w, b, depth),
        BundleResult::NotFound(n) => serialize_not_found(w, n, depth),
        BundleResult::AmbiguousStableId(a) => serialize_ambiguous(w, a, depth),
        BundleResult::Exhausted(e) => serialize_exhausted(w, e, depth),
        BundleResult::UnknownCrossFile(u) => serialize_unknown_cross_file(w, u, depth),
    }
}

fn serialize_bundle(w: &mut impl Write, b: &Bundle, depth: usize) -> io::Result<()> {
    indent(w, depth)?;
    write!(w, "(bundle")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(version {})", b.version)?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(path {})", escape_string(&b.path.to_string_lossy()))?;
    write!(w, "\n")?;

    // Cells (sorted by stable_id)
    let mut cells = b.cells.clone();
    cells.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    indent(w, depth + 1)?;
    write!(w, "(cells")?;
    if cells.is_empty() {
        write!(w, ")")?;
    } else {
        for cell in &cells {
            write!(w, "\n")?;
            serialize_ast_cell(w, cell, depth + 2)?;
        }
        write!(w, "\n")?;
        indent(w, depth + 1)?;
        write!(w, ")")?;
    }
    write!(w, "\n")?;

    // Omitted (sorted by stable_id)
    let mut omitted = b.omitted.clone();
    omitted.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    indent(w, depth + 1)?;
    write!(w, "(omitted")?;
    if omitted.is_empty() {
        write!(w, ")")?;
    } else {
        for om in &omitted {
            write!(w, "\n")?;
            serialize_omitted(w, om, depth + 2)?;
        }
        write!(w, "\n")?;
        indent(w, depth + 1)?;
        write!(w, ")")?;
    }
    write!(w, "\n")?;

    serialize_provenance(w, &b.provenance, depth + 1)?;
    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn serialize_ast_cell(w: &mut impl Write, cell: &AstCell, depth: usize) -> io::Result<()> {
    indent(w, depth)?;
    write!(w, "(cell")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(stable_id {})", escape_string(&cell.stable_id))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(kind {})", escape_string(&cell.kind))?;
    write!(w, "\n")?;
    if let Some(name) = &cell.name {
        indent(w, depth + 1)?;
        write!(w, "(name {})", escape_string(name))?;
        write!(w, "\n")?;
    }
    indent(w, depth + 1)?;
    write!(w, "(range {} {})", cell.byte_range.0, cell.byte_range.1)?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(estimated_tokens {})", cell.estimated_tokens)?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(confidence {})", cell.confidence.as_str())?;
    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn serialize_omitted(w: &mut impl Write, om: &OmittedChunk, depth: usize) -> io::Result<()> {
    indent(w, depth)?;
    write!(w, "(omitted_chunk")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(stable_id {})", escape_string(&om.stable_id))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(reason {})", escape_string(&om.reason))?;
    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn serialize_not_found(w: &mut impl Write, n: &NotFound, depth: usize) -> io::Result<()> {
    indent(w, depth)?;
    write!(w, "(not_found")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(path {})", escape_string(&n.path.to_string_lossy()))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(stable_id {})", escape_string(&n.stable_id))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(reason {})", escape_string(&n.reason))?;
    write!(w, "\n")?;
    serialize_provenance(w, &n.provenance, depth + 1)?;
    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn serialize_ambiguous(
    w: &mut impl Write,
    a: &AmbiguousStableId,
    depth: usize,
) -> io::Result<()> {
    indent(w, depth)?;
    write!(w, "(ambiguous_stable_id")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(path {})", escape_string(&a.path.to_string_lossy()))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(stable_id {})", escape_string(&a.stable_id))?;
    write!(w, "\n")?;

    // Candidates (sorted by anchor_byte, then stable_id)
    let mut candidates = a.candidates.clone();
    candidates.sort_by(|a, b| a.anchor_byte.cmp(&b.anchor_byte).then_with(|| a.kind.cmp(&b.kind)));
    indent(w, depth + 1)?;
    write!(w, "(candidates")?;
    for cand in &candidates {
        write!(w, "\n")?;
        serialize_candidate(w, cand, depth + 2)?;
    }
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, ")")?;
    write!(w, "\n")?;

    indent(w, depth + 1)?;
    write!(w, "(reason {})", escape_string(&a.reason))?;
    write!(w, "\n")?;
    serialize_provenance(w, &a.provenance, depth + 1)?;
    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn serialize_candidate(w: &mut impl Write, c: &Candidate, depth: usize) -> io::Result<()> {
    indent(w, depth)?;
    write!(w, "(candidate")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(anchor_byte {})", c.anchor_byte)?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(kind {})", escape_string(&c.kind))?;
    write!(w, "\n")?;
    if let Some(name) = &c.name {
        indent(w, depth + 1)?;
        write!(w, "(name {})", escape_string(name))?;
        write!(w, "\n")?;
    }
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn serialize_exhausted(w: &mut impl Write, e: &Exhausted, depth: usize) -> io::Result<()> {
    indent(w, depth)?;
    write!(w, "(exhausted")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(path {})", escape_string(&e.path.to_string_lossy()))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(stable_id {})", escape_string(&e.stable_id))?;
    write!(w, "\n")?;

    // Omitted (sorted by stable_id)
    let mut omitted = e.omitted.clone();
    omitted.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    indent(w, depth + 1)?;
    write!(w, "(omitted")?;
    for om in &omitted {
        write!(w, "\n")?;
        serialize_omitted(w, om, depth + 2)?;
    }
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, ")")?;
    write!(w, "\n")?;

    serialize_provenance(w, &e.provenance, depth + 1)?;
    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn serialize_unknown_cross_file(
    w: &mut impl Write,
    u: &UnknownCrossFile,
    depth: usize,
) -> io::Result<()> {
    indent(w, depth)?;
    write!(w, "(unknown_cross_file")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(path {})", escape_string(&u.path.to_string_lossy()))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(stable_id {})", escape_string(&u.stable_id))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(reason {})", escape_string(&u.reason))?;
    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn serialize_provenance(w: &mut impl Write, p: &Provenance, depth: usize) -> io::Result<()> {
    indent(w, depth)?;
    write!(w, "(provenance")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(strategy {})", escape_string(&p.strategy))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(confidence {})", p.confidence.as_str())?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(graph_snapshot_id {})", escape_string(&p.graph_snapshot_id))?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(orientation_freshness {})", escape_string(&p.orientation_freshness))?;
    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn indent(w: &mut impl Write, depth: usize) -> io::Result<()> {
    for _ in 0..depth {
        write!(w, "  ")?;
    }
    Ok(())
}

/// Escape a string for canonical S-expression output.
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
                    // Control characters outside the escape subset are rejected.
                    result.push_str("\u{fffd}");
                } else {
                    result.push(c);
                }
            }
        }
    }
    result.push('"');
    result
}

/// Serialize an invalidation output to canonical S-expression bytes.
pub fn invalidation_to_sexpr(output: &crate::schema::InvalidationOutput) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    invalidation_to_sexpr_inner(&mut buf, output, 0)?;
    buf.push(b'\n');
    Ok(buf)
}

fn invalidation_to_sexpr_inner(
    w: &mut impl std::io::Write,
    output: &crate::schema::InvalidationOutput,
    depth: usize,
) -> std::io::Result<()> {
    indent(w, depth)?;
    write!(w, "(invalidation")?;
    write!(w, "\n")?;

    // Schema version from meta
    indent(w, depth + 1)?;
    write!(
        w,
        "(schema_version {})",
        escape_string(&output.meta.schema_version)
    )?;
    write!(w, "\n")?;

    // Group records by status and sort by stable_id within each group
    let mut affected: Vec<_> = output
        .records
        .iter()
        .filter(|r| r.status == crate::schema::InvalidationStatus::Affected)
        .collect();
    let mut added: Vec<_> = output
        .records
        .iter()
        .filter(|r| r.status == crate::schema::InvalidationStatus::Added)
        .collect();
    let mut removed: Vec<_> = output
        .records
        .iter()
        .filter(|r| r.status == crate::schema::InvalidationStatus::Removed)
        .collect();
    let mut unchanged: Vec<_> = output
        .records
        .iter()
        .filter(|r| r.status == crate::schema::InvalidationStatus::Unchanged)
        .collect();

    affected.sort_by(|a, b| a.chunk.stable_id.cmp(&b.chunk.stable_id));
    added.sort_by(|a, b| a.chunk.stable_id.cmp(&b.chunk.stable_id));
    removed.sort_by(|a, b| a.chunk.stable_id.cmp(&b.chunk.stable_id));
    unchanged.sort_by(|a, b| a.chunk.stable_id.cmp(&b.chunk.stable_id));

    serialize_invalidation_bucket(w, "affected", &affected, depth + 1)?;
    write!(w, "\n")?;
    serialize_invalidation_bucket(w, "added", &added, depth + 1)?;
    write!(w, "\n")?;
    serialize_invalidation_bucket(w, "removed", &removed, depth + 1)?;
    write!(w, "\n")?;
    serialize_invalidation_bucket(w, "unchanged", &unchanged, depth + 1)?;
    write!(w, "\n")?;

    // Changed ranges
    indent(w, depth + 1)?;
    write!(w, "(changed_ranges")?;
    if output.changed_ranges.is_empty() {
        write!(w, ")")?;
    } else {
        for range in &output.changed_ranges {
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(w, "((start {}) (end {}))", range.start, range.end)?;
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
    indent(w, depth + 2)?;
    write!(
        w,
        "(schema_version {})",
        escape_string(&output.meta.schema_version)
    )?;
    write!(w, "\n")?;
    if let Some(path) = &output.meta.source_path {
        indent(w, depth + 2)?;
        write!(w, "(source_path {})", escape_string(&path.to_string_lossy()))?;
        write!(w, "\n")?;
    }
    indent(w, depth + 2)?;
    write!(w, "(total_chunks {})", output.meta.total_chunks)?;
    write!(w, "\n")?;
    indent(w, depth + 2)?;
    write!(
        w,
        "(total_estimated_tokens {})",
        output.meta.total_estimated_tokens
    )?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, ")")?;

    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

fn confidence_str(c: crate::schema::Confidence) -> &'static str {
    match c {
        crate::schema::Confidence::Exact => "exact",
        crate::schema::Confidence::High => "high",
        crate::schema::Confidence::Medium => "medium",
        crate::schema::Confidence::Low => "low",
    }
}

fn invalidation_reason_str(r: crate::schema::InvalidationReason) -> &'static str {
    match r {
        crate::schema::InvalidationReason::ChangedRangeOverlap => "changed_range_overlap",
        crate::schema::InvalidationReason::ContentChanged => "content_changed",
        crate::schema::InvalidationReason::AddedChunk => "added_chunk",
        crate::schema::InvalidationReason::RemovedChunk => "removed_chunk",
        crate::schema::InvalidationReason::NoChangeDetected => "no_change_detected",
        crate::schema::InvalidationReason::DegradedMatching => "degraded_matching",
    }
}

fn match_strategy_str(m: crate::schema::MatchStrategy) -> &'static str {
    match m {
        crate::schema::MatchStrategy::StableId => "stable_id",
        crate::schema::MatchStrategy::ContentComparison => "content_comparison",
        crate::schema::MatchStrategy::TextualRangeOverlap => "textual_range_overlap",
        crate::schema::MatchStrategy::EditRangeOverlap => "edit_range_overlap",
        crate::schema::MatchStrategy::Unmatched => "unmatched",
    }
}

fn serialize_invalidation_bucket(
    w: &mut impl std::io::Write,
    name: &str,
    records: &[&crate::schema::InvalidationRecord],
    depth: usize,
) -> std::io::Result<()> {
    indent(w, depth)?;
    write!(w, "({}", name)?;
    if records.is_empty() {
        write!(w, ")")?;
    } else {
        for record in records {
            write!(w, "\n")?;
            indent(w, depth + 1)?;
            write!(w, "(")?;
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(
                w,
                "(stable_id {})",
                escape_string(&record.chunk.stable_id.0)
            )?;
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(w, "(kind {})", escape_string(&record.chunk.kind))?;
            write!(w, "\n")?;
            if let Some(name) = &record.chunk.name {
                indent(w, depth + 2)?;
                write!(w, "(name {})", escape_string(name))?;
                write!(w, "\n")?;
            }
            indent(w, depth + 2)?;
            write!(
                w,
                "(path {})",
                escape_string(&record.chunk.id.path.to_string_lossy())
            )?;
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(
                w,
                "(byte_range {} {})",
                record.chunk.byte_range.start, record.chunk.byte_range.end
            )?;
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(
                w,
                "(estimated_tokens {})",
                record.chunk.estimated_tokens
            )?;
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(
                w,
                "(confidence {})",
                escape_string(confidence_str(record.chunk.confidence))
            )?;
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(
                w,
                "(reason {})",
                escape_string(invalidation_reason_str(record.reason))
            )?;
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(
                w,
                "(match_strategy {})",
                escape_string(match_strategy_str(record.match_strategy))
            )?;
            write!(w, "\n")?;
            if !record.changed_ranges.is_empty() {
                indent(w, depth + 2)?;
                write!(w, "(changed_ranges")?;
                for range in &record.changed_ranges {
                    write!(w, "\n")?;
                    indent(w, depth + 3)?;
                    write!(w, "((start {}) (end {}))", range.start, range.end)?;
                }
                write!(w, "\n")?;
                indent(w, depth + 2)?;
                write!(w, ")")?;
                write!(w, "\n")?;
            }
            indent(w, depth + 1)?;
            write!(w, ")")?;
        }
        write!(w, "\n")?;
        indent(w, depth)?;
        write!(w, ")")?;
    }
    Ok(())
}

/// Serialize an orientation block to canonical S-expression bytes.
pub fn orientation_to_sexpr(block: &crate::orientation::OrientationBlock) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    orientation_to_sexpr_inner(&mut buf, block, 0)?;
    buf.push(b'\n');
    Ok(buf)
}

fn orientation_to_sexpr_inner(
    w: &mut impl std::io::Write,
    block: &crate::orientation::OrientationBlock,
    depth: usize,
) -> std::io::Result<()> {
    indent(w, depth)?;
    write!(w, "(orientation")?;
    write!(w, "\n")?;

    indent(w, depth + 1)?;
    write!(
        w,
        "(schema_version {})",
        escape_string(&block.schema_version)
    )?;
    write!(w, "\n")?;

    indent(w, depth + 1)?;
    write!(
        w,
        "(graph_snapshot_id {})",
        escape_string(&block.graph_snapshot_id.0)
    )?;
    write!(w, "\n")?;

    // Stats
    indent(w, depth + 1)?;
    write!(w, "(stats")?;
    write!(w, "\n")?;
    indent(w, depth + 2)?;
    write!(w, "(file_count {})", block.stats.file_count)?;
    write!(w, "\n")?;
    indent(w, depth + 2)?;
    write!(w, "(symbol_count {})", block.stats.symbol_count)?;
    write!(w, "\n")?;
    indent(w, depth + 2)?;
    write!(w, "(language_count {})", block.stats.language_count)?;
    write!(w, "\n")?;
    indent(w, depth + 2)?;
    write!(w, "(edge_count {})", block.stats.edge_count)?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, ")")?;
    write!(w, "\n")?;

    // Top referenced
    indent(w, depth + 1)?;
    write!(w, "(top_referenced")?;
    if block.top_referenced.is_empty() {
        write!(w, ")")?;
    } else {
        for tr in &block.top_referenced {
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(w, "((symbol_path {})", escape_string(&tr.symbol_path))?;
            write!(w, " (path {})", escape_string(&tr.path))?;
            write!(w, " (stable_id {})", escape_string(&tr.stable_id))?;
            write!(w, " (inbound_refs {}))", tr.inbound_refs)?;
        }
        write!(w, "\n")?;
        indent(w, depth + 1)?;
        write!(w, ")")?;
    }
    write!(w, "\n")?;

    // Entry points
    indent(w, depth + 1)?;
    write!(w, "(entry_points")?;
    if block.entry_points.is_empty() {
        write!(w, ")")?;
    } else {
        for ep in &block.entry_points {
            write!(w, "\n")?;
            indent(w, depth + 2)?;
            write!(w, "((symbol_path {})", escape_string(&ep.symbol_path))?;
            write!(w, " (path {})", escape_string(&ep.path))?;
            write!(w, " (stable_id {}))", escape_string(&ep.stable_id))?;
        }
        write!(w, "\n")?;
        indent(w, depth + 1)?;
        write!(w, ")")?;
    }
    write!(w, "\n")?;

    // Reserved postprocess fields
    indent(w, depth + 1)?;
    match &block.god_nodes {
        crate::orientation::OrientationField::PostprocessUnavailable => {
            write!(w, "(god_nodes postprocess_unavailable)")?;
        }
        crate::orientation::OrientationField::Computed { status, nodes } => {
            write!(w, "(god_nodes (computation_status {})", escape_string(status))?;
            for node in nodes {
                write!(w, "\n")?;
                indent(w, depth + 2)?;
                write!(
                    w,
                    "((rank {}) (stable_id {}) (path {}))",
                    node.rank,
                    escape_string(&node.stable_id),
                    escape_string(&node.path)
                )?;
            }
            write!(w, ")")?;
        }
    }
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(communities postprocess_unavailable)")?;
    write!(w, "\n")?;
    indent(w, depth + 1)?;
    write!(w, "(architecture_summary postprocess_unavailable)")?;

    // Budget truncated
    if let Some(trunc) = &block.budget_truncated {
        write!(w, "\n")?;
        indent(w, depth + 1)?;
        write!(
            w,
            "(budget_truncated true (reason {}) (omitted",
            escape_string(&trunc.reason)
        )?;
        for item in &trunc.omitted {
            write!(w, " {}", escape_string(item))?;
        }
        write!(w, "))")?;
    }

    write!(w, "\n")?;
    indent(w, depth)?;
    write!(w, ")")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        AmbiguousStableId, AstCell, Bundle, Candidate, Confidence, Exhausted, NotFound,
        OmittedChunk, Provenance, UnknownCrossFile,
    };

    #[test]
    fn deterministic_bundle_serialization() {
        let bundle = Bundle {
            version: 1,
            path: "src/lib.rs".into(),
            cells: vec![
                AstCell {
                    stable_id: "named:abc".to_string(),
                    kind: "function_item".to_string(),
                    name: Some("foo".to_string()),
                    byte_range: (0, 23),
                    estimated_tokens: 6,
                    confidence: Confidence::Exact,
                },
                AstCell {
                    stable_id: "named:def".to_string(),
                    kind: "struct_item".to_string(),
                    name: Some("Bar".to_string()),
                    byte_range: (25, 50),
                    estimated_tokens: 4,
                    confidence: Confidence::Exact,
                },
            ],
            omitted: vec![],
            provenance: Provenance::new("sig_tier_bundle", Confidence::Exact),
        };

        let result = BundleResult::Bundle(bundle);
        let bytes1 = serialize(&result).unwrap();
        let bytes2 = serialize(&result).unwrap();
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn bundle_with_omitted_sorts_stable_ids() {
        let bundle = Bundle {
            version: 1,
            path: "test.rs".into(),
            cells: vec![
                AstCell {
                    stable_id: "named:z".to_string(),
                    kind: "function_item".to_string(),
                    name: Some("z".to_string()),
                    byte_range: (0, 10),
                    estimated_tokens: 2,
                    confidence: Confidence::Exact,
                },
                AstCell {
                    stable_id: "named:a".to_string(),
                    kind: "function_item".to_string(),
                    name: Some("a".to_string()),
                    byte_range: (10, 20),
                    estimated_tokens: 2,
                    confidence: Confidence::Exact,
                },
            ],
            omitted: vec![
                OmittedChunk {
                    stable_id: "named:m".to_string(),
                    reason: "over_budget".to_string(),
                },
            ],
            provenance: Provenance::new("sig_tier_bundle", Confidence::Exact),
        };

        let bytes = serialize(&BundleResult::Bundle(bundle)).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // Cells should be sorted by stable_id: a before z
        let a_pos = s.find("(stable_id \"named:a\")").unwrap();
        let z_pos = s.find("(stable_id \"named:z\")").unwrap();
        assert!(a_pos < z_pos);
    }

    #[test]
    fn not_found_serializes_with_zero_confidence() {
        let not_found = NotFound {
            path: "src/lib.rs".into(),
            stable_id: "named:missing".to_string(),
            reason: "no chunk with this stable_id found in file".to_string(),
            provenance: Provenance::new("stable_id_lookup", Confidence::Low),
        };

        let bytes = serialize(&BundleResult::NotFound(not_found)).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("(not_found"));
        assert!(s.contains("(confidence low)"));
        // U1: "unknown" is no longer the sentinel for missing graph; verify field presence.
        assert!(s.contains("(graph_snapshot_id "));
        assert!(s.contains("(orientation_freshness "));
        assert!(
            s.contains("(orientation_freshness \"unknown\")")
                || s.contains("(orientation_freshness \"fresh\")")
                || s.contains("(orientation_freshness \"stale\")"),
            "orientation_freshness must be one of {{unknown,fresh,stale}}"
        );
    }

    #[test]
    fn ambiguous_stable_id_serializes_candidates() {
        let ambiguous = AmbiguousStableId {
            path: "src/lib.rs".into(),
            stable_id: "named:dup".to_string(),
            candidates: vec![
                Candidate {
                    anchor_byte: 0,
                    kind: "function_item".to_string(),
                    name: Some("foo".to_string()),
                },
                Candidate {
                    anchor_byte: 45,
                    kind: "function_item".to_string(),
                    name: Some("foo".to_string()),
                },
            ],
            reason: "multiple chunks share this stable_id".to_string(),
            provenance: Provenance::new("stable_id_lookup", Confidence::Low),
        };

        let bytes = serialize(&BundleResult::AmbiguousStableId(ambiguous)).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("(ambiguous_stable_id"));
        assert!(s.contains("(candidate"));
        assert!(s.contains("(anchor_byte 0)"));
        assert!(s.contains("(anchor_byte 45)"));
    }

    #[test]
    fn exhausted_serializes_omitted_in_order() {
        let exhausted = Exhausted {
            path: "src/lib.rs".into(),
            stable_id: "named:foo".to_string(),
            omitted: vec![
                OmittedChunk {
                    stable_id: "named:foo".to_string(),
                    reason: "over_budget".to_string(),
                },
            ],
            provenance: Provenance::new("sig_tier_bundle", Confidence::Exact),
        };

        let bytes = serialize(&BundleResult::Exhausted(exhausted)).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("(exhausted"));
        assert!(s.contains("(omitted_chunk"));
        assert!(s.contains("(reason \"over_budget\")"));
    }

    #[test]
    fn string_escaping_covers_required_chars() {
        let bundle = Bundle {
            version: 1,
            path: "test.rs".into(),
            cells: vec![AstCell {
                stable_id: "named:test".to_string(),
                kind: "function_item".to_string(),
                name: Some("foo\\bar\"baz\nqux\t".to_string()),
                byte_range: (0, 10),
                estimated_tokens: 2,
                confidence: Confidence::Exact,
            }],
            omitted: vec![],
            provenance: Provenance::new("sig_tier_bundle", Confidence::Exact),
        };

        let bytes = serialize(&BundleResult::Bundle(bundle)).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("foo\\\\bar\\\"baz\\nqux\\t"));
    }

    #[test]
    fn unknown_cross_file_serializes() {
        let unknown = UnknownCrossFile {
            path: "src/lib.rs".into(),
            stable_id: "named:foo".to_string(),
            reason: "v1-non-goal".to_string(),
        };

        let bytes = serialize(&BundleResult::UnknownCrossFile(unknown)).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("(unknown_cross_file"));
        assert!(s.contains("(reason \"v1-non-goal\")"));
    }

    #[test]
    fn computed_god_nodes_serializes_correctly() {
        use crate::graph::postprocess::GodNode;
        use crate::orientation::{
            OrientationBlock, OrientationField, OrientationStats,
        };

        let block = OrientationBlock {
            schema_version: "r2-2026-04-26".to_string(),
            graph_snapshot_id: crate::graph::snapshot::GraphSnapshotId("snap123".to_string()),
            stats: OrientationStats {
                file_count: 1,
                symbol_count: 2,
                language_count: 1,
                edge_count: 1,
            },
            top_referenced: vec![],
            entry_points: vec![],
            god_nodes: OrientationField::Computed {
                status: "computed".to_string(),
                nodes: vec![
                    GodNode {
                        rank: 1,
                        stable_id: "named:foo".to_string(),
                        path: "src/lib.rs".to_string(),
                    },
                    GodNode {
                        rank: 2,
                        stable_id: "named:bar".to_string(),
                        path: "src/main.rs".to_string(),
                    },
                ],
            },
            communities: OrientationField::PostprocessUnavailable,
            architecture_summary: OrientationField::PostprocessUnavailable,
            budget_truncated: None,
        };

        let bytes = crate::sexpr::orientation_to_sexpr(&block).unwrap();
        let s = String::from_utf8(bytes).unwrap();

        assert!(s.contains("(god_nodes (computation_status \"computed\")"));
        assert!(s.contains("((rank 1) (stable_id \"named:foo\") (path \"src/lib.rs\"))"));
        assert!(s.contains("((rank 2) (stable_id \"named:bar\") (path \"src/main.rs\"))"));
    }

    #[test]
    fn empty_computed_god_nodes_serializes_without_children() {
        use crate::orientation::{
            OrientationBlock, OrientationField, OrientationStats,
        };

        let block = OrientationBlock {
            schema_version: "r2-2026-04-26".to_string(),
            graph_snapshot_id: crate::graph::snapshot::GraphSnapshotId("snap456".to_string()),
            stats: OrientationStats {
                file_count: 0,
                symbol_count: 0,
                language_count: 0,
                edge_count: 0,
            },
            top_referenced: vec![],
            entry_points: vec![],
            god_nodes: OrientationField::Computed {
                status: "computed".to_string(),
                nodes: vec![],
            },
            communities: OrientationField::PostprocessUnavailable,
            architecture_summary: OrientationField::PostprocessUnavailable,
            budget_truncated: None,
        };

        let bytes = crate::sexpr::orientation_to_sexpr(&block).unwrap();
        let s = String::from_utf8(bytes).unwrap();

        assert!(s.contains("(god_nodes (computation_status \"computed\"))"));
        // Ensure there are no child lists after the status
        let god_nodes_start = s.find("(god_nodes").unwrap();
        let god_nodes_end = s[god_nodes_start..].find(")").unwrap() + god_nodes_start;
        let god_nodes_section = &s[god_nodes_start..=god_nodes_end];
        assert!(
            !god_nodes_section.contains("(rank"),
            "empty god_nodes should not contain rank entries"
        );
    }

    #[test]
    fn invalidation_output_deterministic_serialization() {
        use std::path::PathBuf;
        use crate::identity::StableId;
        use crate::schema::{
            ByteRange, ChunkId, ChunkRecord, Confidence, Diagnostic, InvalidationOutput,
            InvalidationReason, InvalidationRecord, InvalidationStatus, MatchStrategy, OutputMeta,
        };

        let chunk1 = ChunkRecord {
            id: ChunkId {
                path: PathBuf::from("src/lib.rs"),
                kind: "function_item".to_string(),
                name: Some("foo".to_string()),
                anchor_byte: 0,
            },
            stable_id: StableId("named:foo".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            byte_range: ByteRange { start: 0, end: 23 },
            estimated_tokens: 6,
            confidence: Confidence::Exact,
            depth: 0,
            parent: None,
        };

        let chunk2 = ChunkRecord {
            id: ChunkId {
                path: PathBuf::from("src/lib.rs"),
                kind: "struct_item".to_string(),
                name: Some("Bar".to_string()),
                anchor_byte: 25,
            },
            stable_id: StableId("named:bar".to_string()),
            kind: "struct_item".to_string(),
            name: Some("Bar".to_string()),
            byte_range: ByteRange { start: 25, end: 50 },
            estimated_tokens: 4,
            confidence: Confidence::High,
            depth: 0,
            parent: None,
        };

        let output = InvalidationOutput {
            records: vec![
                InvalidationRecord {
                    status: InvalidationStatus::Affected,
                    chunk: chunk1.clone(),
                    old_chunk: Some(chunk1.clone()),
                    reason: InvalidationReason::ContentChanged,
                    match_strategy: MatchStrategy::StableId,
                    confidence: Confidence::Exact,
                    changed_ranges: vec![ByteRange { start: 10, end: 20 }],
                },
                InvalidationRecord {
                    status: InvalidationStatus::Unchanged,
                    chunk: chunk2.clone(),
                    old_chunk: Some(chunk2.clone()),
                    reason: InvalidationReason::NoChangeDetected,
                    match_strategy: MatchStrategy::StableId,
                    confidence: Confidence::High,
                    changed_ranges: vec![],
                },
            ],
            affected: vec![chunk1.clone()],
            added: vec![],
            removed: vec![],
            unchanged: vec![chunk2.clone()],
            changed_ranges: vec![ByteRange { start: 10, end: 20 }],
            diagnostics: vec![Diagnostic::info("1 affected chunk(s)")],
            meta: OutputMeta {
                schema_version: "0.1.0".to_string(),
                source_path: Some(PathBuf::from("src/lib.rs")),
                total_chunks: 2,
                total_estimated_tokens: 10,
            },
        };

        let bytes1 = invalidation_to_sexpr(&output).unwrap();
        let bytes2 = invalidation_to_sexpr(&output).unwrap();
        assert_eq!(bytes1, bytes2, "serialization must be byte-stable");

        let s = String::from_utf8(bytes1).unwrap();
        assert!(s.contains("(invalidation"));
        assert!(s.contains("(schema_version \"0.1.0\")"));
        assert!(s.contains("(affected"));
        assert!(s.contains("(stable_id \"named:foo\")"));
        assert!(s.contains("(reason \"content_changed\")"));
        assert!(s.contains("(match_strategy \"stable_id\")"));
        assert!(s.contains("(confidence \"exact\")"));
        assert!(s.contains("(unchanged"));
        assert!(s.contains("(stable_id \"named:bar\")"));
        assert!(s.contains("(reason \"no_change_detected\")"));
        assert!(s.contains("(confidence \"high\")"));
        assert!(s.contains("(changed_ranges"));
        assert!(s.contains("(meta"));
        assert!(s.contains("(total_chunks 2)"));
    }

    #[test]
    fn invalidation_empty_buckets_serializes() {
        use std::path::PathBuf;
        use crate::schema::{
            InvalidationOutput, OutputMeta,
        };

        let output = InvalidationOutput {
            records: vec![],
            affected: vec![],
            added: vec![],
            removed: vec![],
            unchanged: vec![],
            changed_ranges: vec![],
            diagnostics: vec![],
            meta: OutputMeta {
                schema_version: "0.1.0".to_string(),
                source_path: Some(PathBuf::from("src/lib.rs")),
                total_chunks: 0,
                total_estimated_tokens: 0,
            },
        };

        let bytes = invalidation_to_sexpr(&output).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("(affected)"));
        assert!(s.contains("(added)"));
        assert!(s.contains("(removed)"));
        assert!(s.contains("(unchanged)"));
        assert!(s.contains("(changed_ranges)"));
    }

    #[test]
    fn invalidation_sorts_by_stable_id() {
        use std::path::PathBuf;
        use crate::identity::StableId;
        use crate::schema::{
            ByteRange, ChunkId, ChunkRecord, Confidence, InvalidationOutput,
            InvalidationReason, InvalidationRecord, InvalidationStatus, MatchStrategy, OutputMeta,
        };

        let chunk_z = ChunkRecord {
            id: ChunkId {
                path: PathBuf::from("src/lib.rs"),
                kind: "function_item".to_string(),
                name: Some("z".to_string()),
                anchor_byte: 0,
            },
            stable_id: StableId("named:z".to_string()),
            kind: "function_item".to_string(),
            name: Some("z".to_string()),
            byte_range: ByteRange { start: 0, end: 10 },
            estimated_tokens: 2,
            confidence: Confidence::Exact,
            depth: 0,
            parent: None,
        };

        let chunk_a = ChunkRecord {
            id: ChunkId {
                path: PathBuf::from("src/lib.rs"),
                kind: "function_item".to_string(),
                name: Some("a".to_string()),
                anchor_byte: 10,
            },
            stable_id: StableId("named:a".to_string()),
            kind: "function_item".to_string(),
            name: Some("a".to_string()),
            byte_range: ByteRange { start: 10, end: 20 },
            estimated_tokens: 2,
            confidence: Confidence::Exact,
            depth: 0,
            parent: None,
        };

        let output = InvalidationOutput {
            records: vec![
                InvalidationRecord {
                    status: InvalidationStatus::Affected,
                    chunk: chunk_z.clone(),
                    old_chunk: Some(chunk_z.clone()),
                    reason: InvalidationReason::ContentChanged,
                    match_strategy: MatchStrategy::StableId,
                    confidence: Confidence::Exact,
                    changed_ranges: vec![],
                },
                InvalidationRecord {
                    status: InvalidationStatus::Affected,
                    chunk: chunk_a.clone(),
                    old_chunk: Some(chunk_a.clone()),
                    reason: InvalidationReason::ContentChanged,
                    match_strategy: MatchStrategy::StableId,
                    confidence: Confidence::Exact,
                    changed_ranges: vec![],
                },
            ],
            affected: vec![chunk_z.clone(), chunk_a.clone()],
            added: vec![],
            removed: vec![],
            unchanged: vec![],
            changed_ranges: vec![],
            diagnostics: vec![],
            meta: OutputMeta {
                schema_version: "0.1.0".to_string(),
                source_path: Some(PathBuf::from("src/lib.rs")),
                total_chunks: 2,
                total_estimated_tokens: 4,
            },
        };

        let bytes = invalidation_to_sexpr(&output).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        let a_pos = s.find("(stable_id \"named:a\")").unwrap();
        let z_pos = s.find("(stable_id \"named:z\")").unwrap();
        assert!(a_pos < z_pos, "records should be sorted by stable_id");
    }
}

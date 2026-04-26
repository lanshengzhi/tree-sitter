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
    write!(w, "(god_nodes postprocess_unavailable)")?;
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
}

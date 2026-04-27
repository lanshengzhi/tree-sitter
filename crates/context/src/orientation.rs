//! Thin orientation block derived from a graph snapshot.
//!
//! Produces deterministic, byte-stable output for LLM consumption.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::chunk::estimate_tokens;
use crate::graph::snapshot::{GraphSnapshot, GraphSnapshotId};

/// Current orientation schema version. Bumps invalidate prior contracts.
pub const ORIENTATION_SCHEMA_VERSION: &str = "r2-2026-04-26";

/// A thin orientation block with deterministic stats and reserved postprocess fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrientationBlock {
    pub schema_version: String,
    pub graph_snapshot_id: GraphSnapshotId,
    pub stats: OrientationStats,
    pub top_referenced: Vec<TopReferenced>,
    pub entry_points: Vec<EntryPoint>,
    pub god_nodes: OrientationField,
    pub communities: OrientationField,
    pub architecture_summary: OrientationField,
    /// Budget truncation metadata, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_truncated: Option<BudgetTruncated>,
}

/// Deterministic stats derived from the snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrientationStats {
    pub file_count: usize,
    pub symbol_count: usize,
    pub language_count: usize,
    pub edge_count: usize,
}

/// A top-referenced symbol with cross-file inbound ref count.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Ord, PartialOrd)]
pub struct TopReferenced {
    pub symbol_path: String,
    pub path: String,
    pub stable_id: String,
    pub inbound_refs: usize,
}

/// An entry point symbol (public, no inbound refs).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Ord, PartialOrd)]
pub struct EntryPoint {
    pub symbol_path: String,
    pub path: String,
    pub stable_id: String,
}

/// Reserved postprocess field placeholder.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrientationField {
    PostprocessUnavailable,
    Computed {
        #[serde(rename = "computation_status")]
        status: String,
        nodes: Vec<crate::graph::postprocess::GodNode>,
    },
}

/// Budget truncation metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BudgetTruncated {
    pub reason: String,
    pub omitted: Vec<String>,
}

/// Build a deterministic, byte-stable orientation block from a snapshot.
#[must_use]
pub fn build_orientation(
    snapshot: &GraphSnapshot,
    budget: Option<usize>,
    god_nodes: Option<Vec<crate::graph::postprocess::GodNode>>,
) -> OrientationBlock {
    let stats = build_stats(snapshot);
    let top_referenced = build_top_referenced(snapshot);
    let entry_points = build_entry_points(snapshot);

    let god_nodes_field = match god_nodes {
        Some(nodes) => OrientationField::Computed {
            status: "computed".to_string(),
            nodes,
        },
        None => OrientationField::PostprocessUnavailable,
    };

    let mut block = OrientationBlock {
        schema_version: ORIENTATION_SCHEMA_VERSION.to_string(),
        graph_snapshot_id: snapshot.snapshot_id.clone(),
        stats,
        top_referenced,
        entry_points,
        god_nodes: god_nodes_field,
        communities: OrientationField::PostprocessUnavailable,
        architecture_summary: OrientationField::PostprocessUnavailable,
        budget_truncated: None,
    };

    if let Some(b) = budget {
        apply_budget(&mut block, b);
    }

    block
}

fn build_stats(snapshot: &GraphSnapshot) -> OrientationStats {
    let file_count = snapshot.files.len();
    let symbol_count: usize = snapshot.files.iter().map(|f| f.symbols.len()).sum();
    // Language count: number of unique file extensions
    let mut languages = std::collections::HashSet::new();
    for file in &snapshot.files {
        if let Some(ext) = file.path.extension() {
            languages.insert(ext.to_string_lossy().to_string());
        }
    }
    let language_count = languages.len();
    let edge_count = snapshot.edges.len();

    OrientationStats {
        file_count,
        symbol_count,
        language_count,
        edge_count,
    }
}

fn build_top_referenced(snapshot: &GraphSnapshot) -> Vec<TopReferenced> {
    // Count cross-file inbound refs (confirmed edges where target is in a different file)
    let mut inbound_counts: HashMap<(&std::path::Path,&str,&str), usize> = HashMap::new();
    for edge in &snapshot.edges {
        if edge.status == crate::graph::snapshot::EdgeStatus::Confirmed
            && edge.source.path != edge.target.path
        {
            let key = (
                edge.target.path.as_path(),
                edge.target.stable_id.0.as_str(),
                edge.kind.as_str(),
            );
            *inbound_counts.entry(key).or_insert(0) += 1;
        }
    }

    // Build TopReferenced entries from counts
    let mut entries: Vec<TopReferenced> = inbound_counts
        .into_iter()
        .map(|((path, stable_id, _kind), count)| TopReferenced {
            symbol_path: stable_id.to_string(),
            path: path.to_string_lossy().to_string(),
            stable_id: stable_id.to_string(),
            inbound_refs: count,
        })
        .collect();

    // Sort by (-inbound_refs, path, stable_id)
    entries.sort_by(|a, b| {
        b.inbound_refs
            .cmp(&a.inbound_refs)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.stable_id.cmp(&b.stable_id))
    });

    // Take top N (default 20)
    let top_n = entries.len().min(20);
    entries.truncate(top_n);
    entries
}

fn build_entry_points(snapshot: &GraphSnapshot) -> Vec<EntryPoint> {
    // Find public definitions with no inbound refs from the same file
    let mut entry_points = Vec::new();
    let mut has_inbound: HashMap<(&std::path::Path,&str), bool> = HashMap::new();

    // Mark nodes that have inbound refs (any edge targeting them)
    for edge in &snapshot.edges {
        let key = (edge.target.path.as_path(), edge.target.stable_id.0.as_str());
        has_inbound.insert(key, true);
    }

    for file in &snapshot.files {
        for sym in &file.symbols {
            if sym.is_definition {
                let key = (file.path.as_path(), sym.node_handle.stable_id.0.as_str());
                let is_entry = !has_inbound.get(&key).copied().unwrap_or(false);
                if is_entry {
                    entry_points.push(EntryPoint {
                        symbol_path: sym.node_handle.stable_id.0.clone(),
                        path: file.path.to_string_lossy().to_string(),
                        stable_id: sym.node_handle.stable_id.0.clone(),
                    });
                }
            }
        }
    }

    // Sort by (path, stable_id)
    entry_points.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.stable_id.cmp(&b.stable_id))
    });

    entry_points
}

fn apply_budget(block: &mut OrientationBlock, budget: usize) {
    // Estimate tokens of full block
    let json = serde_json::to_string(block).unwrap_or_default();
    let estimated = estimate_tokens(json.len());

    if estimated <= budget {
        return;
    }

    // Truncation priority: stats > top_referenced > entry_points
    let mut omitted = Vec::new();

    // Try dropping entry_points first
    if !block.entry_points.is_empty() {
        omitted.push("entry_points".to_string());
        block.entry_points.clear();
        let json = serde_json::to_string(block).unwrap_or_default();
        if estimate_tokens(json.len()) <= budget {
            block.budget_truncated = Some(BudgetTruncated {
                reason: "budget_exhausted".to_string(),
                omitted,
            });
            return;
        }
    }

    // Try dropping top_referenced
    if !block.top_referenced.is_empty() {
        omitted.push("top_referenced".to_string());
        block.top_referenced.clear();
        let json = serde_json::to_string(block).unwrap_or_default();
        if estimate_tokens(json.len()) <= budget {
            block.budget_truncated = Some(BudgetTruncated {
                reason: "budget_exhausted".to_string(),
                omitted,
            });
            return;
        }
    }

    // If still over budget, keep only stats
    block.top_referenced.clear();
    block.entry_points.clear();
    block.budget_truncated = Some(BudgetTruncated {
        reason: "budget_exhausted".to_string(),
        omitted: vec![
            "top_referenced".to_string(),
            "entry_points".to_string(),
        ],
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::snapshot::{
        GraphFile, GraphNode, GraphSnapshot, GraphSnapshotId,
        GraphSymbol, GRAPH_SCHEMA_VERSION,
    };
    use crate::schema::{ByteRange, Confidence};
    use crate::identity::StableId;
    use std::path::PathBuf;

    fn make_test_snapshot() -> GraphSnapshot {
        let node_a = GraphNode {
            path: PathBuf::from("src/a.rs"),
            stable_id: StableId("named:target".to_string()),
            kind: "function_item".to_string(),
            name: Some("target".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 10 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        };

        let node_b = GraphNode {
            path: PathBuf::from("src/b.rs"),
            stable_id: StableId("named:caller".to_string()),
            kind: "function_item".to_string(),
            name: Some("caller".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 10 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        };

        let sym_a = GraphSymbol {
            name: "target".to_string(),
            syntax_type: "function".to_string(),
            byte_range: ByteRange { start: 0, end: 10 },
            is_definition: true,
            node_handle: (&node_a).into(),
            confidence: Confidence::Exact,
        };

        let sym_b = GraphSymbol {
            name: "caller".to_string(),
            syntax_type: "function".to_string(),
            byte_range: ByteRange { start: 0, end: 10 },
            is_definition: true,
            node_handle: (&node_b).into(),
            confidence: Confidence::Exact,
        };

        let file_a = GraphFile {
            path: PathBuf::from("src/a.rs"),
            content_hash: None,
            nodes: vec![node_a],
            symbols: vec![sym_a],
            diagnostics: vec![],
        };

        let file_b = GraphFile {
            path: PathBuf::from("src/b.rs"),
            content_hash: None,
            nodes: vec![node_b],
            symbols: vec![sym_b],
            diagnostics: vec![],
        };

        GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId("test123".to_string()),
            files: vec![file_a, file_b],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        }
    }

    #[test]
    fn orientation_block_has_expected_fields() {
        let snapshot = make_test_snapshot();
        let block = build_orientation(&snapshot, None, None);

        assert_eq!(block.schema_version, ORIENTATION_SCHEMA_VERSION);
        assert_eq!(block.graph_snapshot_id.0, "test123");
        assert_eq!(block.stats.file_count, 2);
        assert_eq!(block.stats.symbol_count, 2);
        assert_eq!(block.god_nodes, OrientationField::PostprocessUnavailable);
        assert_eq!(block.communities, OrientationField::PostprocessUnavailable);
        assert_eq!(
            block.architecture_summary,
            OrientationField::PostprocessUnavailable
        );
    }

    #[test]
    fn byte_stability_across_two_builds() {
        let snapshot = make_test_snapshot();
        let block1 = build_orientation(&snapshot, None, None);
        let block2 = build_orientation(&snapshot, None, None);

        let json1 = serde_json::to_string(&block1).unwrap();
        let json2 = serde_json::to_string(&block2).unwrap();
        assert_eq!(json1, json2, "orientation block must be byte-stable");
    }

    #[test]
    fn budget_truncation_drops_entry_points_first() {
        let snapshot = make_test_snapshot();
        let block = build_orientation(&snapshot, Some(50), None);

        assert!(
            block.budget_truncated.is_some(),
            "budget_truncated must be set when budget is exhausted"
        );
        let trunc = block.budget_truncated.unwrap();
        assert_eq!(trunc.reason, "budget_exhausted");
        assert!(trunc.omitted.contains(&"entry_points".to_string()));
    }
}

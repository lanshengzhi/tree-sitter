//! Deterministic PageRank computation for cross-file god-nodes.

use std::collections::BTreeMap;

use crate::graph::postprocess::GodNode;
use crate::graph::snapshot::GraphSnapshot;

const DAMPING: f64 = 0.85;
const ITERATIONS: usize = 30;
const TOP_N: usize = 20;
const TIE_BREAK_EPSILON: f64 = 1e-12;

/// Compute god-nodes via deterministic PageRank on cross-file edges.
///
/// - 30 iterations, uniform initialization (1/N)
/// - Dangling nodes redistribute uniformly
/// - Tie-break by (path, stable_id) lexicographic when |score_i - score_j| < 1e-12
/// - Returns top-20 nodes with ranks 1..K (K <= 20)
#[must_use]
pub fn compute_god_nodes(snapshot: &GraphSnapshot) -> Vec<GodNode> {
    let n = snapshot.files.iter().map(|f| f.nodes.len()).sum::<usize>();
    if n == 0 {
        return vec![];
    }

    // 1. Build sorted node index map: BTreeMap ensures deterministic ordering
    let mut node_index: BTreeMap<(String, String), usize> = BTreeMap::new();
    for file in &snapshot.files {
        for node in &file.nodes {
            let key = (
                node.path.to_string_lossy().to_string(),
                node.stable_id.0.clone(),
            );
            let idx = node_index.len();
            node_index.entry(key).or_insert(idx);
        }
    }

    let num_nodes = node_index.len();
    if num_nodes == 0 {
        return vec![];
    }

    // Build reverse lookup: index -> (path, stable_id)
    let mut index_to_key: Vec<(String, String)> = vec![(String::new(), String::new()); num_nodes];
    for ((path, stable_id), idx) in &node_index {
        index_to_key[*idx] = (path.clone(), stable_id.clone());
    }

    // 2. Filter cross-file edges and build adjacency lists
    let mut out_degree: Vec<usize> = vec![0; num_nodes];
    let mut inbound: Vec<Vec<usize>> = vec![vec![]; num_nodes];

    for edge in &snapshot.edges {
        if edge.source.path == edge.target.path {
            continue; // skip intra-file edges
        }
        let src_key = (
            edge.source.path.to_string_lossy().to_string(),
            edge.source.stable_id.0.clone(),
        );
        let tgt_key = (
            edge.target.path.to_string_lossy().to_string(),
            edge.target.stable_id.0.clone(),
        );

        let src_idx = match node_index.get(&src_key) {
            Some(&idx) => idx,
            None => continue,
        };
        let tgt_idx = match node_index.get(&tgt_key) {
            Some(&idx) => idx,
            None => continue,
        };

        out_degree[src_idx] += 1;
        inbound[tgt_idx].push(src_idx);
    }

    // 3. Power iteration
    let mut ranks = vec![1.0 / num_nodes as f64; num_nodes];
    let base = (1.0 - DAMPING) / num_nodes as f64;

    for _ in 0..ITERATIONS {
        let dangling_sum: f64 = ranks
            .iter()
            .enumerate()
            .filter(|(i, _)| out_degree[*i] == 0)
            .map(|(_, r)| r)
            .sum();
        let dangling_term = DAMPING * dangling_sum / num_nodes as f64;

        let mut new_ranks = vec![0.0; num_nodes];
        for v in 0..num_nodes {
            let mut sum = 0.0;
            for &w in &inbound[v] {
                sum += ranks[w] / out_degree[w] as f64;
            }
            new_ranks[v] = base + dangling_term + DAMPING * sum;
        }
        ranks = new_ranks;
    }

    // 4. Sort by (-score, path, stable_id) with tie-break
    let mut scored: Vec<(usize, f64, String, String)> = ranks
        .into_iter()
        .enumerate()
        .map(|(idx, score)| {
            let (path, stable_id) = index_to_key[idx].clone();
            (idx, score, path, stable_id)
        })
        .collect();

    scored.sort_by(|a, b| {
        let score_diff = b.1 - a.1;
        if score_diff.abs() < TIE_BREAK_EPSILON {
            // Tie: sort by (path, stable_id) ascending
            a.2.cmp(&b.2).then_with(|| a.3.cmp(&b.3))
        } else if score_diff > 0.0 {
            std::cmp::Ordering::Greater // b has higher score, so b comes first
        } else {
            std::cmp::Ordering::Less
        }
    });

    // 5. Take top-N and assign ranks
    let top_k = scored.len().min(TOP_N);
    let mut god_nodes = Vec::with_capacity(top_k);
    for i in 0..top_k {
        let (_, _, path, stable_id) = &scored[i];
        god_nodes.push(GodNode {
            rank: i + 1,
            stable_id: stable_id.clone(),
            path: path.clone(),
        });
    }

    god_nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::snapshot::{
        EdgeStatus, GraphEdge, GraphFile, GraphNode, GraphSnapshot, GraphSnapshotId,
        GraphNodeHandle, GRAPH_SCHEMA_VERSION,
    };
    use crate::schema::{ByteRange, Confidence};
    use crate::identity::StableId;
    use std::path::PathBuf;

    fn make_node(path: &str, stable_id: &str) -> GraphNode {
        GraphNode {
            path: PathBuf::from(path),
            stable_id: StableId(stable_id.to_string()),
            kind: "function_item".to_string(),
            name: Some(stable_id.to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 10 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        }
    }

    fn make_edge(src: (&str, &str), tgt: (&str, &str)) -> GraphEdge {
        GraphEdge {
            source: GraphNodeHandle {
                path: PathBuf::from(src.0),
                stable_id: StableId(src.1.to_string()),
                anchor_byte: 0,
            },
            target: GraphNodeHandle {
                path: PathBuf::from(tgt.0),
                stable_id: StableId(tgt.1.to_string()),
                anchor_byte: 0,
            },
            kind: "call".to_string(),
            status: EdgeStatus::Confirmed,
            confidence: Confidence::Exact,
            candidates: vec![],
        }
    }

    fn snapshot_with_nodes(nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) -> GraphSnapshot {
        let mut files: Vec<GraphFile> = Vec::new();
        for node in &nodes {
            let file_idx = files.iter().position(|f| f.path == node.path);
            match file_idx {
                Some(idx) => {
                    files[idx].nodes.push(node.clone());
                }
                None => {
                    files.push(GraphFile {
                        path: node.path.clone(),
                        content_hash: None,
                        nodes: vec![node.clone()],
                        symbols: vec![],
                        diagnostics: vec![],
                    });
                }
            }
        }

        GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files,
            edges,
            diagnostics: vec![],
            meta: None,
        }
    }

    #[test]
    fn empty_graph() {
        let snapshot = snapshot_with_nodes(vec![], vec![]);
        let nodes = compute_god_nodes(&snapshot);
        assert!(nodes.is_empty());
    }

    #[test]
    fn single_node_no_edges() {
        let snapshot = snapshot_with_nodes(
            vec![make_node("src/lib.rs", "named:foo")],
            vec![],
        );
        let nodes = compute_god_nodes(&snapshot);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].rank, 1);
        assert_eq!(nodes[0].stable_id, "named:foo");
    }

    #[test]
    fn star_graph() {
        // Center node called by all peripheral nodes
        let center = make_node("src/lib.rs", "named:center");
        let p1 = make_node("src/a.rs", "named:p1");
        let p2 = make_node("src/b.rs", "named:p2");
        let p3 = make_node("src/c.rs", "named:p3");

        let edges = vec![
            make_edge(("src/a.rs", "named:p1"), ("src/lib.rs", "named:center")),
            make_edge(("src/b.rs", "named:p2"), ("src/lib.rs", "named:center")),
            make_edge(("src/c.rs", "named:p3"), ("src/lib.rs", "named:center")),
        ];

        let snapshot = snapshot_with_nodes(
            vec![center.clone(), p1, p2, p3],
            edges,
        );
        let nodes = compute_god_nodes(&snapshot);
        assert_eq!(nodes[0].stable_id, "named:center");
        assert_eq!(nodes[0].rank, 1);
    }

    #[test]
    fn chain_graph() {
        // A -> B -> C -> D
        let a = make_node("src/a.rs", "named:a");
        let b = make_node("src/b.rs", "named:b");
        let c = make_node("src/c.rs", "named:c");
        let d = make_node("src/d.rs", "named:d");

        let edges = vec![
            make_edge(("src/a.rs", "named:a"), ("src/b.rs", "named:b")),
            make_edge(("src/b.rs", "named:b"), ("src/c.rs", "named:c")),
            make_edge(("src/c.rs", "named:c"), ("src/d.rs", "named:d")),
        ];

        let snapshot = snapshot_with_nodes(vec![a, b.clone(), c.clone(), d], edges);
        let nodes = compute_god_nodes(&snapshot);
        // In standard PageRank with dangling redistribution, D receives from C
        // and redistributes uniformly, often ending with highest rank.
        // The key invariant: A (source) has the lowest rank.
        let ids: Vec<String> = nodes.iter().map(|n| n.stable_id.clone()).collect();
        let pos_a = ids.iter().position(|id| id == "named:a").unwrap();
        let pos_b = ids.iter().position(|id| id == "named:b").unwrap();
        let pos_c = ids.iter().position(|id| id == "named:c").unwrap();
        let pos_d = ids.iter().position(|id| id == "named:d").unwrap();
        assert!(pos_a > pos_b, "A should rank lower than B");
        assert!(pos_a > pos_c, "A should rank lower than C");
        assert!(pos_a > pos_d, "A should rank lower than D");
    }

    #[test]
    fn dangling_node() {
        // A -> B, C has no outbound edges (dangling)
        let a = make_node("src/a.rs", "named:a");
        let b = make_node("src/b.rs", "named:b");
        let c = make_node("src/c.rs", "named:c");

        let edges = vec![
            make_edge(("src/a.rs", "named:a"), ("src/b.rs", "named:b")),
        ];

        let snapshot = snapshot_with_nodes(vec![a, b, c], edges);
        let nodes = compute_god_nodes(&snapshot);
        // Should not panic; C gets redistributed rank
        assert_eq!(nodes.len(), 3);
    }

    #[test]
    fn self_loop() {
        // A -> A (self-loop)
        let a = make_node("src/lib.rs", "named:a");

        let edges = vec![
            make_edge(("src/lib.rs", "named:a"), ("src/lib.rs", "named:a")),
        ];

        let snapshot = snapshot_with_nodes(vec![a], edges);
        let nodes = compute_god_nodes(&snapshot);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].stable_id, "named:a");
    }

    #[test]
    fn duplicate_edges() {
        // Two edges A -> B
        let a = make_node("src/a.rs", "named:a");
        let b = make_node("src/b.rs", "named:b");

        let edges = vec![
            make_edge(("src/a.rs", "named:a"), ("src/b.rs", "named:b")),
            make_edge(("src/a.rs", "named:a"), ("src/b.rs", "named:b")),
        ];

        let snapshot = snapshot_with_nodes(vec![a, b], edges);
        let nodes = compute_god_nodes(&snapshot);
        // B should rank higher due to multi-edge
        assert_eq!(nodes[0].stable_id, "named:b");
    }

    #[test]
    fn determinism_contract() {
        // Same snapshot run twice -> identical results
        let a = make_node("src/a.rs", "named:a");
        let b = make_node("src/b.rs", "named:b");
        let c = make_node("src/c.rs", "named:c");

        let edges = vec![
            make_edge(("src/a.rs", "named:a"), ("src/b.rs", "named:b")),
            make_edge(("src/b.rs", "named:b"), ("src/c.rs", "named:c")),
            make_edge(("src/c.rs", "named:c"), ("src/a.rs", "named:a")),
        ];

        let snapshot = snapshot_with_nodes(vec![a, b, c], edges);
        let run1 = compute_god_nodes(&snapshot);
        let run2 = compute_god_nodes(&snapshot);
        assert_eq!(run1, run2);
    }

    #[test]
    fn tie_break_by_path_and_stable_id() {
        // Two nodes with symmetric scores (same graph structure)
        let a = make_node("src/a.rs", "named:a");
        let b = make_node("src/b.rs", "named:b");

        // No edges -> both have same score after 30 iterations
        let snapshot = snapshot_with_nodes(vec![a, b], vec![]);
        let nodes = compute_god_nodes(&snapshot);
        // Tie broken by (path, stable_id): a.rs < b.rs
        assert_eq!(nodes[0].stable_id, "named:a");
        assert_eq!(nodes[1].stable_id, "named:b");
    }

}

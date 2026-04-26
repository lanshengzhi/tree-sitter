//! Per-file graph extraction from chunk and symbol outputs.

use std::collections::HashMap;
use std::path::Path;

use xxhash_rust::xxh3::xxh3_128;

use crate::chunk::ChunkOutput;
use crate::identity::StableId;
use crate::schema::{ChunkRecord, Confidence, Diagnostic, SymbolRecord};
use crate::symbols::SymbolsOutput;

use super::snapshot::{GraphFile, GraphNode, GraphSymbol};

/// Extract a `GraphFile` from parsed outputs.
///
/// `path` must be repo-relative. `source` is the full file bytes.
/// `content_hash` is an optional pre-computed hash for the entire file.
/// When `tags_unavailable` is true, a diagnostic is added but chunk
/// records are still emitted.
#[must_use]
pub fn extract_graph_file(
    path: impl AsRef<Path>,
    source: &[u8],
    chunk_output: &ChunkOutput,
    symbol_output: Option<&SymbolsOutput>,
    content_hash: Option<String>,
    tags_unavailable: bool,
) -> GraphFile {
    let path = path.as_ref().to_path_buf();
    let mut diagnostics = chunk_output.diagnostics.clone();

    if tags_unavailable {
        diagnostics.push(Diagnostic::warn(
            "tags configuration unavailable; symbols omitted, chunks preserved",
        ));
    }

    let mut nodes: Vec<GraphNode> = chunk_output
        .chunks
        .iter()
        .map(|chunk| chunk_to_node(chunk, source))
        .collect();

    // Detect duplicate stable_ids within this file and downgrade confidence
    let mut stable_id_counts: HashMap<StableId, usize> = HashMap::new();
    for node in &nodes {
        *stable_id_counts.entry(node.stable_id.clone()).or_insert(0) += 1;
    }
    for node in &mut nodes {
        if stable_id_counts.get(&node.stable_id).copied().unwrap_or(0) > 1 {
            node.confidence = Confidence::Low;
        }
    }

    let symbols: Vec<GraphSymbol> = symbol_output
        .map(|s| {
            s.symbols
                .iter()
                .map(|sym| symbol_to_graph_symbol(sym, &path))
                .collect()
        })
        .unwrap_or_default();

    diagnostics.extend(
        symbol_output
            .map(|s| s.diagnostics.clone())
            .unwrap_or_default(),
    );

    GraphFile {
        path,
        content_hash,
        nodes,
        symbols,
        diagnostics,
    }
}

fn chunk_to_node(chunk: &ChunkRecord, source: &[u8]) -> GraphNode {
    let text = &source[chunk.byte_range.start..chunk.byte_range.end];
    let content_hash = Some(format!("{:032x}", xxh3_128(text)));
    let signature_hash = Some(compute_signature_hash(chunk));

    GraphNode {
        path: chunk.id.path.clone(),
        stable_id: chunk.stable_id.clone(),
        kind: chunk.kind.clone(),
        name: chunk.name.clone(),
        anchor_byte: chunk.id.anchor_byte,
        byte_range: chunk.byte_range,
        signature_hash,
        content_hash,
        confidence: chunk.confidence,
    }
}

fn compute_signature_hash(chunk: &ChunkRecord) -> String {
    let mut input = String::new();
    input.push_str(&chunk.stable_id.0);
    input.push('\0');
    input.push_str(&chunk.kind);
    input.push('\0');
    if let Some(name) = &chunk.name {
        input.push_str(name);
    }
    format!("{:032x}", xxh3_128(input.as_bytes()))
}

fn symbol_to_graph_symbol(sym: &SymbolRecord, path: &Path) -> GraphSymbol {
    GraphSymbol {
        name: sym.name.clone(),
        syntax_type: sym.syntax_type.clone(),
        byte_range: sym.byte_range,
        is_definition: sym.is_definition,
        node_handle: super::snapshot::GraphNodeHandle {
            path: path.to_path_buf(),
            stable_id: StableId(format!(
                "named:{}",
                sym.name
            )),
            anchor_byte: sym.byte_range.start,
        },
        confidence: sym.confidence,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tree_sitter::Parser;

    use super::*;
    use crate::chunk::{ChunkOptions, chunks_for_tree};

    fn rust_parser() -> Parser {
        let raw = unsafe { tree_sitter_rust::LANGUAGE.into_raw()() };
        let language: tree_sitter::Language = unsafe { std::mem::transmute(raw) };
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        parser
    }

    #[test]
    fn happy_path_rust_function() {
        let mut parser = rust_parser();
        let source = b"fn foo() {}";
        let tree = parser.parse(source, None).unwrap();

        let chunk_output = chunks_for_tree(
            &tree,
            Path::new("src/lib.rs"),
            source,
            &ChunkOptions::default(),
        );

        let graph_file = extract_graph_file(
            "src/lib.rs",
            source,
            &chunk_output,
            None,
            Some("file_hash".to_string()),
            false,
        );

        assert_eq!(graph_file.path, Path::new("src/lib.rs"));
        assert!(graph_file.content_hash.is_some());
        assert!(!graph_file.nodes.is_empty());

        let foo_node = graph_file
            .nodes
            .iter()
            .find(|n| n.name.as_deref() == Some("foo"));
        assert!(foo_node.is_some(), "should have a node named foo");
        let foo_node = foo_node.unwrap();
        assert_eq!(foo_node.kind, "function_item");
        assert!(foo_node.signature_hash.is_some());
        assert!(foo_node.content_hash.is_some());
        assert_eq!(foo_node.confidence, Confidence::Exact);
    }

    #[test]
    fn syntax_errors_downgrade_confidence() {
        let mut parser = rust_parser();
        let source = b"fn foo() { let x = ";
        let tree = parser.parse(source, None).unwrap();

        let chunk_output = chunks_for_tree(
            &tree,
            Path::new("src/lib.rs"),
            source,
            &ChunkOptions::default(),
        );

        let graph_file = extract_graph_file(
            "src/lib.rs",
            source,
            &chunk_output,
            None,
            None,
            false,
        );

        assert!(
            graph_file.diagnostics.iter().any(|d| d.message.contains("syntax errors")),
            "should have syntax error diagnostic"
        );
        assert!(
            graph_file.nodes.iter().any(|n| n.confidence == Confidence::Low),
            "should have low-confidence nodes"
        );
    }

    #[test]
    fn missing_tags_adds_diagnostic() {
        let mut parser = rust_parser();
        let source = b"fn foo() {}";
        let tree = parser.parse(source, None).unwrap();

        let chunk_output = chunks_for_tree(
            &tree,
            Path::new("src/lib.rs"),
            source,
            &ChunkOptions::default(),
        );

        let graph_file = extract_graph_file(
            "src/lib.rs",
            source,
            &chunk_output,
            None,
            None,
            true,
        );

        assert!(
            graph_file
                .diagnostics
                .iter()
                .any(|d| d.message.contains("tags configuration unavailable")),
            "should have tags unavailable diagnostic"
        );
        assert!(!graph_file.nodes.is_empty(), "chunks should still be present");
    }

    #[test]
    fn duplicate_stable_ids_get_low_confidence() {
        let mut parser = rust_parser();
        let source = b"
fn foo() {}
fn foo() {}
";
        let tree = parser.parse(source, None).unwrap();

        let chunk_output = chunks_for_tree(
            &tree,
            Path::new("src/lib.rs"),
            source,
            &ChunkOptions::default(),
        );

        let graph_file = extract_graph_file(
            "src/lib.rs",
            source,
            &chunk_output,
            None,
            None,
            false,
        );

        let foo_nodes: Vec<_> = graph_file
            .nodes
            .iter()
            .filter(|n| n.name.as_deref() == Some("foo"))
            .collect();

        assert_eq!(foo_nodes.len(), 2, "should have two foo nodes");
        assert_eq!(foo_nodes[0].stable_id, foo_nodes[1].stable_id);
        assert_eq!(foo_nodes[0].confidence, Confidence::Low);
        assert_eq!(foo_nodes[1].confidence, Confidence::Low);
    }
}

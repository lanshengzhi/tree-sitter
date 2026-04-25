use std::borrow::Cow;
use std::path::Path;

use tree_sitter::{Node, Tree};

use crate::identity::StableId;
use crate::schema::{ByteRange, ChunkId, ChunkRecord, Confidence};

use crate::schema::Diagnostic;

/// Options for producing semantic chunks from a syntax tree.
#[derive(Clone, Debug)]
pub struct ChunkOptions {
    /// The maximum desired token count for a chunk.
    ///
    /// Milestone 5 records this value but does not split large syntax nodes yet.
    pub max_tokens: usize,
    /// Maximum number of chunks to emit. Additional chunk boundaries are ignored.
    pub max_chunks: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            max_tokens: 2_000,
            max_chunks: 1_000,
        }
    }
}

/// Output from a chunking pass.
#[derive(Clone, Debug)]
pub struct ChunkOutput {
    pub chunks: Vec<ChunkRecord>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Produce chunks for a parsed source file.
pub fn chunks_for_tree(
    tree: &Tree,
    path: impl AsRef<Path>,
    source: &[u8],
    options: &ChunkOptions,
) -> ChunkOutput {
    let path = path.as_ref().to_path_buf();
    let mut chunker = Chunker {
        path: &path,
        source,
        chunks: Vec::new(),
        options,
        confidence: if tree.root_node().has_error() {
            Confidence::Low
        } else {
            Confidence::Exact
        },
    };

    chunker.visit(tree.root_node(), 0, None);

    let mut diagnostics = Vec::new();

    if tree.root_node().has_error() {
        diagnostics.push(Diagnostic::warn(
            "parse tree contains syntax errors; chunk confidence downgraded",
        ));
    }

    if chunker.chunks.is_empty() {
        chunker.push_chunk(tree.root_node(), 0, None);
        diagnostics.push(Diagnostic::info(
            "no recognized chunk boundaries found; falling back to entire file as single chunk",
        ));
    }

    if chunker.chunks.len() >= options.max_chunks {
        diagnostics.push(Diagnostic::warn(format!(
            "chunk limit ({}) reached; some boundaries were omitted",
            options.max_chunks,
        )));
    }

    ChunkOutput {
        chunks: chunker.chunks,
        diagnostics,
    }
}

struct Chunker<'a> {
    path: &'a Path,
    source: &'a [u8],
    chunks: Vec<ChunkRecord>,
    options: &'a ChunkOptions,
    confidence: Confidence,
}

impl Chunker<'_> {
    fn visit(&mut self, node: Node, depth: usize, parent: Option<ChunkId>) {
        let this_id = if is_chunk_boundary_kind(node.kind()) {
            self.push_chunk(node, depth, parent.clone())
        } else {
            None
        };

        let child_parent = this_id.or(parent);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, depth + 1, child_parent.clone());
        }
    }

    fn push_chunk(&mut self, node: Node, depth: usize, parent: Option<ChunkId>) -> Option<ChunkId> {
        if self.chunks.len() >= self.options.max_chunks {
            return None;
        }

        let byte_range = ByteRange::from(node.start_byte()..node.end_byte());
        let byte_len = byte_range.len();
        let name = node_name(node, self.source).map(Cow::into_owned);

        let id = ChunkId {
            path: self.path.to_path_buf(),
            kind: node.kind().to_string(),
            name: name.clone(),
            anchor_byte: byte_range.start,
        };

        let stable_id = StableId::compute(
            self.path,
            node.kind(),
            name.as_deref(),
            parent.as_ref(),
            self.source,
            &byte_range,
        );

        self.chunks.push(ChunkRecord {
            id: id.clone(),
            stable_id,
            kind: node.kind().to_string(),
            name,
            byte_range,
            estimated_tokens: estimate_tokens(byte_len),
            confidence: self.confidence,
            depth,
            parent,
        });

        Some(id)
    }
}

fn is_chunk_boundary_kind(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "const_item"
            | "enum_declaration"
            | "enum_item"
            | "function_declaration"
            | "function_item"
            | "generator_function_declaration"
            | "impl_item"
            | "interface_declaration"
            | "macro_definition"
            | "method_definition"
            | "mod_item"
            | "module"
            | "static_item"
            | "struct_declaration"
            | "struct_item"
            | "trait_item"
            | "type_alias_declaration"
            | "type_item"
    ) || kind.ends_with("_function")
        || kind.ends_with("_method")
}

fn node_name<'a>(node: Node, source: &'a [u8]) -> Option<Cow<'a, str>> {
    node.child_by_field_name("name")
        .or_else(|| first_named_identifier(node))
        .and_then(|name| name.utf8_text(source).ok())
        .map(Cow::Borrowed)
}

fn first_named_identifier(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).find(|child| {
        matches!(
            child.kind(),
            "identifier" | "property_identifier" | "type_identifier"
        )
    })
}

fn estimate_tokens(byte_len: usize) -> usize {
    byte_len.div_ceil(4).max(1)
}

#[cfg(test)]
mod tests {
    use super::{ChunkOptions, chunks_for_tree, estimate_tokens, is_chunk_boundary_kind};
    use crate::schema::Confidence;
    use std::path::Path;
    use tree_sitter::Parser;

    fn rust_parser() -> Parser {
        let raw = unsafe { tree_sitter_rust::LANGUAGE.into_raw()() };
        let language: tree_sitter::Language = unsafe { std::mem::transmute(raw) };
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        parser
    }

    #[test]
    fn recognizes_common_chunk_boundaries() {
        assert!(is_chunk_boundary_kind("function_item"));
        assert!(is_chunk_boundary_kind("method_definition"));
        assert!(is_chunk_boundary_kind("struct_item"));
        assert!(is_chunk_boundary_kind("class_declaration"));
        assert!(!is_chunk_boundary_kind("identifier"));
        assert!(!is_chunk_boundary_kind("statement_block"));
    }

    #[test]
    fn estimates_tokens_conservatively_from_bytes() {
        assert_eq!(estimate_tokens(0), 1);
        assert_eq!(estimate_tokens(1), 1);
        assert_eq!(estimate_tokens(4), 1);
        assert_eq!(estimate_tokens(5), 2);
    }

    #[test]
    fn records_true_estimated_tokens_even_when_max_tokens_is_smaller() {
        let mut parser = rust_parser();
        let source = b"fn huge() { let value = 1; let other = 2; let third = 3; }";
        let tree = parser.parse(source, None).unwrap();

        let output = chunks_for_tree(
            &tree,
            Path::new("test.rs"),
            source,
            &ChunkOptions {
                max_tokens: 1,
                max_chunks: 1_000,
            },
        );

        assert_eq!(output.chunks.len(), 1);
        assert!(output.chunks[0].estimated_tokens > 1);
    }

    #[test]
    fn parse_errors_emit_diagnostic_and_downgrade_confidence() {
        let mut parser = rust_parser();
        let source = b"fn broken(";
        let tree = parser.parse(source, None).unwrap();

        let output = chunks_for_tree(
            &tree,
            Path::new("test.rs"),
            source,
            &ChunkOptions::default(),
        );

        assert!(
            output
                .diagnostics
                .iter()
                .any(|d| d.message.contains("syntax errors"))
        );
        assert!(
            output
                .chunks
                .iter()
                .all(|chunk| chunk.confidence == Confidence::Low)
        );
    }
}

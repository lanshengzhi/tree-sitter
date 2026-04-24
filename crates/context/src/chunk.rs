use std::borrow::Cow;
use std::path::Path;

use tree_sitter::{Node, Tree};

use crate::schema::{ByteRange, ChunkId, ChunkRecord, Confidence};

/// Options for producing semantic chunks from a syntax tree.
#[derive(Clone, Debug)]
pub struct ChunkOptions {
    /// The maximum desired token count for a chunk.
    ///
    /// Milestone 5 records this value but does not split large syntax nodes yet.
    pub max_tokens: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self { max_tokens: 2_000 }
    }
}

/// Produce chunks for a parsed source file.
pub fn chunks_for_tree(
    tree: &Tree,
    path: impl AsRef<Path>,
    source: &[u8],
    options: &ChunkOptions,
) -> Vec<ChunkRecord> {
    let path = path.as_ref().to_path_buf();
    let mut chunker = Chunker {
        path: &path,
        source,
        chunks: Vec::new(),
        options,
    };

    chunker.visit(tree.root_node());

    if chunker.chunks.is_empty() {
        chunker.push_chunk(tree.root_node(), None);
    }

    chunker.chunks
}

struct Chunker<'a> {
    path: &'a Path,
    source: &'a [u8],
    chunks: Vec<ChunkRecord>,
    options: &'a ChunkOptions,
}

impl<'a> Chunker<'a> {
    fn visit(&mut self, node: Node) {
        let _ = if is_chunk_boundary_kind(node.kind()) {
            Some(self.push_chunk(node, None))
        } else {
            None
        };

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child);
        }
    }

    fn push_chunk(&mut self, node: Node, _parent: Option<ChunkId>) -> ChunkId {
        let byte_range = ByteRange::from(node.start_byte()..node.end_byte());
        let byte_len = byte_range.len();
        let name = node_name(node, self.source).map(Cow::into_owned);

        let id = ChunkId {
            path: self.path.to_path_buf(),
            kind: node.kind().to_string(),
            name: name.clone(),
            anchor_byte: byte_range.start,
        };

        self.chunks.push(ChunkRecord {
            id,
            kind: node.kind().to_string(),
            name,
            byte_range,
            estimated_tokens: estimate_tokens(byte_len).min(self.options.max_tokens.max(1)),
            confidence: Confidence::Exact,
        });

        // Return the id of the chunk we just pushed.
        let idx = self.chunks.len() - 1;
        self.chunks[idx].id.clone()
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
    use super::{estimate_tokens, is_chunk_boundary_kind};

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
}

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

        let (signature_hash, body_hash) = compute_signature_body_hashes(node, self.source);

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
            signature_hash,
            body_hash,
        });

        Some(id)
    }
}

/// Compute signature and body hashes for a syntax node.
///
/// `signature_hash` = hash of node text excluding body children.
/// `body_hash` = hash of body children text (or same as signature_hash if no body exists).
fn compute_signature_body_hashes(node: Node, source: &[u8]) -> (String, String) {
    use crate::identity::StableDigest;

    let mut body_children: Vec<Node> = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_body_child(child) {
            body_children.push(child);
        }
    }

    if body_children.is_empty() {
        // No body children: signature == body == hash of full node text
        let full_text = &source[node.start_byte()..node.end_byte()];
        let mut digest = StableDigest::new();
        digest.write_field(full_text);
        let hash = format!("{:032x}", digest.finish());
        return (hash.clone(), hash);
    }

    // Collect body byte ranges
    let body_ranges: Vec<std::ops::Range<usize>> = body_children
        .iter()
        .map(|child| child.start_byte()..child.end_byte())
        .collect();

    // Build signature text (full text minus body ranges)
    let mut sig_bytes: Vec<u8> = Vec::new();
    let mut last_end = node.start_byte();
    for range in &body_ranges {
        sig_bytes.extend_from_slice(&source[last_end..range.start]);
        last_end = range.end;
    }
    sig_bytes.extend_from_slice(&source[last_end..node.end_byte()]);

    // Hash signature
    let mut sig_digest = StableDigest::new();
    sig_digest.write_field(&sig_bytes);
    let signature_hash = format!("{:032x}", sig_digest.finish());

    // Build body text
    let mut body_bytes: Vec<u8> = Vec::new();
    for range in &body_ranges {
        body_bytes.extend_from_slice(&source[range.start..range.end]);
    }

    // Hash body
    let mut body_digest = StableDigest::new();
    body_digest.write_field(&body_bytes);
    let body_hash = format!("{:032x}", body_digest.finish());

    (signature_hash, body_hash)
}

/// Heuristic to identify body children of a syntax node.
fn is_body_child(node: Node) -> bool {
    let kind = node.kind();
    let field_name = node.parent().and_then(|p| {
        let child_index = p
            .children(&mut p.walk())
            .position(|c| c.id() == node.id())?;
        p.field_name_for_child(child_index as u32)
    });

    matches!(field_name, Some("body"))
        || kind.ends_with("_body")
        || kind == "block"
        || kind == "statement_block"
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

pub(crate) fn estimate_tokens(byte_len: usize) -> usize {
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

    #[test]
    fn function_with_body_has_different_signature_and_body_hashes() {
        let mut parser = rust_parser();
        let source = b"fn foo() { let x = 1; }";
        let tree = parser.parse(source, None).unwrap();

        let output = chunks_for_tree(
            &tree,
            Path::new("test.rs"),
            source,
            &ChunkOptions::default(),
        );

        assert_eq!(output.chunks.len(), 1);
        let chunk = &output.chunks[0];
        assert_ne!(
            chunk.signature_hash, chunk.body_hash,
            "function with body should have different signature and body hashes"
        );
    }

    #[test]
    fn type_alias_without_body_has_equal_hashes() {
        let mut parser = rust_parser();
        let source = b"type Foo = i32;";
        let tree = parser.parse(source, None).unwrap();

        let output = chunks_for_tree(
            &tree,
            Path::new("test.rs"),
            source,
            &ChunkOptions::default(),
        );

        assert_eq!(output.chunks.len(), 1);
        let chunk = &output.chunks[0];
        assert_eq!(
            chunk.signature_hash, chunk.body_hash,
            "type alias without body children should have signature_hash == body_hash"
        );
    }

    #[test]
    fn nested_function_body_excluded_from_signature() {
        let mut parser = rust_parser();
        let source = b"fn outer() { fn inner() { let x = 1; } }";
        let tree = parser.parse(source, None).unwrap();

        let output = chunks_for_tree(
            &tree,
            Path::new("test.rs"),
            source,
            &ChunkOptions::default(),
        );

        assert_eq!(output.chunks.len(), 2);
        let outer = output.chunks.iter().find(|c| c.name.as_deref() == Some("outer")).unwrap();
        let inner = output.chunks.iter().find(|c| c.name.as_deref() == Some("inner")).unwrap();

        assert_ne!(
            outer.signature_hash, outer.body_hash,
            "outer function should have different signature and body hashes"
        );
        assert_ne!(
            inner.signature_hash, inner.body_hash,
            "inner function should have different signature and body hashes"
        );
    }

    #[test]
    fn hash_fields_roundtrip_through_serialization() {
        let mut parser = rust_parser();
        let source = b"fn foo() { let x = 1; }";
        let tree = parser.parse(source, None).unwrap();

        let output = chunks_for_tree(
            &tree,
            Path::new("test.rs"),
            source,
            &ChunkOptions::default(),
        );

        let chunk = &output.chunks[0];
        let json = serde_json::to_string(chunk).unwrap();
        let deserialized: crate::schema::ChunkRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(chunk.signature_hash, deserialized.signature_hash);
        assert_eq!(chunk.body_hash, deserialized.body_hash);
    }
}

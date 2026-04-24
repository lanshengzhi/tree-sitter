use std::path::Path;

use tree_sitter_tags::{Tag, TagsConfiguration, TagsContext};

use crate::schema::{ByteRange, Confidence, SymbolRecord};

/// Options for symbol extraction.
#[derive(Clone, Debug, Default)]
pub struct SymbolOptions {
    pub max_docs_len: usize,
}

/// Extract symbols from a parsed source file using tags queries.
pub fn symbols_for_tree(
    _tree: &tree_sitter::Tree,
    path: impl AsRef<Path>,
    source: &[u8],
    tags_config: &TagsConfiguration,
    _opts: &SymbolOptions,
) -> Vec<SymbolRecord> {
    let path = path.as_ref().to_path_buf();
    let mut context = TagsContext::new();

    let mut symbols = Vec::new();

    let (tags, _) = match context.generate_tags(tags_config, source, None) {
        Ok(result) => result,
        Err(e) => {
            // Return empty vec on error; caller can add diagnostic.
            eprintln!("tags error: {}", e);
            return symbols;
        }
    };

    for tag in tags {
        let tag = match tag {
            Ok(t) => t,
            Err(e) => {
                eprintln!("tag error: {}", e);
                continue;
            }
        };

        symbols.push(tag_to_record(&tag, &path, source, tags_config));
    }

    symbols
}

fn tag_to_record(
    tag: &Tag,
    path: &Path,
    source: &[u8],
    config: &TagsConfiguration,
) -> SymbolRecord {
    let name = std::str::from_utf8(&source[tag.name_range.clone()])
        .unwrap_or("")
        .to_string();

    let syntax_type = config.syntax_type_name(tag.syntax_type_id).to_string();

    SymbolRecord {
        name,
        syntax_type,
        byte_range: ByteRange::from(tag.range.clone()),
        line_range: ByteRange::from(tag.line_range.clone()),
        docs: tag.docs.clone(),
        is_definition: tag.is_definition,
        path: path.to_path_buf(),
        confidence: Confidence::Exact,
    }
}

#[cfg(test)]
mod tests {
    // Symbol tests require a compiled grammar with tags.scm/locals.scm.
    // Coverage documented for integration testing.
}

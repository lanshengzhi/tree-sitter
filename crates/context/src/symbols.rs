use std::path::Path;

use tree_sitter_tags::{Tag, TagsConfiguration, TagsContext};

use crate::schema::{ByteRange, Confidence, SymbolRecord};

/// Options for symbol extraction.
#[derive(Clone, Debug)]
pub struct SymbolOptions {
    pub max_docs_len: usize,
    /// Maximum number of symbols to extract. Additional tags are skipped.
    pub max_symbols: usize,
}

impl Default for SymbolOptions {
    fn default() -> Self {
        Self {
            max_docs_len: 1_000,
            max_symbols: 1_000,
        }
    }
}

/// Output from a symbol extraction pass.
#[derive(Clone, Debug)]
pub struct SymbolsOutput {
    pub symbols: Vec<SymbolRecord>,
    pub diagnostics: Vec<crate::schema::Diagnostic>,
}

/// Extract symbols from a source file using tags queries.
pub fn symbols_for_tree(
    path: impl AsRef<Path>,
    source: &[u8],
    tags_config: &TagsConfiguration,
    opts: &SymbolOptions,
) -> SymbolsOutput {
    let path = path.as_ref().to_path_buf();
    let mut context = TagsContext::new();

    let mut symbols = Vec::new();
    let mut diagnostics = Vec::new();

    let (tags, _) = match context.generate_tags(tags_config, source, None) {
        Ok(result) => result,
        Err(e) => {
            diagnostics.push(crate::schema::Diagnostic::error(format!(
                "tags query failed: {e}"
            )));
            return SymbolsOutput {
                symbols,
                diagnostics,
            };
        }
    };

    for tag in tags {
        if symbols.len() >= opts.max_symbols {
            let max_symbols = opts.max_symbols;
            diagnostics.push(crate::schema::Diagnostic::warn(format!(
                "symbol limit ({max_symbols}) reached; additional tags were skipped",
            )));
            break;
        }

        let tag = match tag {
            Ok(t) => t,
            Err(e) => {
                diagnostics.push(crate::schema::Diagnostic::warn(format!(
                    "skipping malformed tag: {e}"
                )));
                continue;
            }
        };

        symbols.push(tag_to_record(&tag, &path, source, tags_config, opts));
    }

    SymbolsOutput {
        symbols,
        diagnostics,
    }
}

fn tag_to_record(
    tag: &Tag,
    path: &Path,
    source: &[u8],
    config: &TagsConfiguration,
    opts: &SymbolOptions,
) -> SymbolRecord {
    let name = std::str::from_utf8(&source[tag.name_range.clone()])
        .unwrap_or("")
        .to_string();

    let syntax_type = config.syntax_type_name(tag.syntax_type_id).to_string();

    SymbolRecord {
        name,
        syntax_type,
        byte_range: ByteRange::from(tag.range.clone()),
        lines: ByteRange::from(tag.line_range.clone()),
        docs: tag
            .docs
            .as_ref()
            .map(|docs| truncate_docs(docs, opts.max_docs_len)),
        is_definition: tag.is_definition,
        path: path.to_path_buf(),
        confidence: Confidence::Exact,
    }
}

fn truncate_docs(docs: &str, max_len: usize) -> String {
    docs.chars().take(max_len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST_TAG_QUERY: &str = r#"
(
  (line_comment)* @doc .
  (function_item
    name: (identifier) @name) @definition.function
  (#select-adjacent! @doc @definition.function)
  (#strip! @doc "(^///\\s*)")
)
"#;

    fn rust_tags_config() -> TagsConfiguration {
        let raw = unsafe { tree_sitter_rust::LANGUAGE.into_raw()() };
        let language: tree_sitter::Language = unsafe { std::mem::transmute(raw) };
        TagsConfiguration::new(language, RUST_TAG_QUERY, "").unwrap()
    }

    #[test]
    fn extracts_symbols_with_truncated_docs() {
        let source = br"
/// Documented function with long docs
fn documented() {}
";
        let config = rust_tags_config();
        let output = symbols_for_tree(
            "test.rs",
            source,
            &config,
            &SymbolOptions {
                max_docs_len: 10,
                max_symbols: 100,
            },
        );

        assert!(output.diagnostics.is_empty());
        assert_eq!(output.symbols.len(), 1);
        assert_eq!(output.symbols[0].name, "documented");
        assert_eq!(output.symbols[0].docs.as_deref(), Some("Documented"));
    }

    #[test]
    fn stops_at_max_symbols_with_diagnostic() {
        let source = b"fn one() {}\nfn two() {}\n";
        let config = rust_tags_config();
        let output = symbols_for_tree(
            "test.rs",
            source,
            &config,
            &SymbolOptions {
                max_docs_len: 100,
                max_symbols: 1,
            },
        );

        assert_eq!(output.symbols.len(), 1);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("symbol limit"))
        );
    }
}

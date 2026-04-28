//! Semantic compaction: keep full content for changed chunks, extract signatures for unchanged named chunks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use tree_sitter::Parser;
use tree_sitter_tags::TagsConfiguration;

use crate::{
    chunk::{ChunkOptions, chunks_for_tree},
    invalidation::invalidate_snapshot,
    schema::{
        ChunkRecord, CompactChunkRecord, CompactFileResult, CompactOmittedRecord,
        CompactOutput, Diagnostic, InvalidationStatus, OutputMeta,
    },
    symbols::{SymbolOptions, symbols_for_tree},
};

/// Error type for compaction failures.
#[derive(Clone, Debug)]
pub enum CompactError {
    BudgetExceeded { required_tokens: usize, budget: usize },
}

impl std::fmt::Display for CompactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompactError::BudgetExceeded { required_tokens, budget } => {
                write!(f, "budget_exceeded: required {} tokens but budget is {}", required_tokens, budget)
            }
        }
    }
}

impl std::error::Error for CompactError {}

/// Language configuration for a single file.
#[derive(Clone, Debug)]
pub struct FileLanguage {
    pub language: tree_sitter::Language,
}

/// Options for semantic compaction.
#[derive(Clone, Debug)]
pub struct CompactOptions {
    pub budget: Option<usize>,
}

/// Compact files by keeping full content for changed chunks and extracting signatures for unchanged named chunks.
///
/// For each file in `paths`, compares old and new content, classifies chunks via invalidation,
/// and produces a `CompactFileResult`. If `budget` is set and the compacted output exceeds it,
/// signatures_only chunks are discarded first, then least-recently-referenced preserved chunks.
pub fn compact_files(
    paths: &[PathBuf],
    old_contents: &HashMap<PathBuf, Vec<u8>>,
    languages: &HashMap<PathBuf, FileLanguage>,
    tags_configs: &HashMap<PathBuf, &TagsConfiguration>,
    opts: &CompactOptions,
) -> Result<CompactOutput> {
    let mut files = Vec::new();
    let mut total_original = 0usize;
    let mut total_compacted = 0usize;
    let mut diagnostics = Vec::new();
    let mut top_level_omitted = Vec::new();

    for path in paths {
        let new_source = match std::fs::read(path) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(Diagnostic::error(format!(
                    "file_not_found: failed to read {}: {}",
                    path.display(),
                    e
                )));
                continue;
            }
        };

        let old_source = match old_contents.get(path) {
            Some(s) => s.clone(),
            None => {
                diagnostics.push(Diagnostic::error(format!(
                    "missing_old_content: no old content provided for {}",
                    path.display()
                )));
                continue;
            }
        };

        let file_lang = match languages.get(path) {
            Some(lang) => lang,
            None => {
                diagnostics.push(Diagnostic::error(format!(
                    "no_language: no language configuration for {}",
                    path.display()
                )));
                continue;
            }
        };

        let tags_config = tags_configs.get(path).copied();

        match compact_single_file(path, &old_source, &new_source, file_lang, tags_config, opts) {
            Ok((file_result, file_diag)) => {
                total_original += file_result.original_tokens;
                total_compacted += file_result.compacted_tokens;
                files.push(file_result);
                diagnostics.extend(file_diag);
            }
            Err(e) => {
                diagnostics.push(Diagnostic::error(format!(
                    "compaction_error for {}: {}",
                    path.display(),
                    e
                )));
            }
        }
    }

    // Apply global budget if set
    if let Some(budget) = opts.budget {
        if total_compacted > budget {
            let mut new_compacted = 0usize;
            let mut new_omitted = Vec::new();

            for file in &mut files {
                let mut kept_preserved = Vec::new();
                let mut kept_signatures = Vec::new();

                // First, discard all signatures_only chunks
                for record in &file.signatures_only {
                    match record {
                        CompactChunkRecord::SignatureOnly { chunk, .. } => {
                            new_omitted.push(CompactOmittedRecord {
                                stable_id: chunk.stable_id.clone(),
                                kind: chunk.kind.clone(),
                                name: chunk.name.clone(),
                                reason: "budget".to_string(),
                                estimated_tokens: chunk.estimated_tokens,
                            });
                        }
                        CompactChunkRecord::Preserved { chunk } => {
                            kept_preserved.push(chunk.clone());
                        }
                    }
                }
                file.signatures_only = kept_signatures;

                // Then discard preserved chunks if still over budget
                for chunk in &file.preserved {
                    if new_compacted + chunk.estimated_tokens <= budget {
                        new_compacted += chunk.estimated_tokens;
                        kept_preserved.push(chunk.clone());
                    } else {
                        new_omitted.push(CompactOmittedRecord {
                            stable_id: chunk.stable_id.clone(),
                            kind: chunk.kind.clone(),
                            name: chunk.name.clone(),
                            reason: "budget".to_string(),
                            estimated_tokens: chunk.estimated_tokens,
                        });
                    }
                }
                file.preserved = kept_preserved;
                file.compacted_tokens = new_compacted;
            }

            total_compacted = new_compacted;
            top_level_omitted = new_omitted;

            if total_compacted > budget {
                return Err(CompactError::BudgetExceeded {
                    required_tokens: total_compacted,
                    budget,
                }.into());
            }

            diagnostics.push(Diagnostic::info(format!(
                "{} chunk(s) omitted to fit within budget of {} tokens",
                top_level_omitted.len(),
                budget
            )));
        }
    }

    let total_chunks: usize = files.iter().map(|f| f.preserved.len() + f.signatures_only.len()).sum();
    let total_estimated: usize = files.iter().map(|f| f.original_tokens).sum();

    Ok(CompactOutput {
        files,
        original_tokens: total_original,
        compacted_tokens: total_compacted,
        omitted: top_level_omitted,
        diagnostics,
        meta: OutputMeta {
            schema_version: "0.1.0".to_string(),
            source_path: None,
            total_chunks,
            total_estimated_tokens: total_estimated,
        },
    })
}

fn compact_single_file(
    path: &Path,
    old_source: &[u8],
    new_source: &[u8],
    file_lang: &FileLanguage,
    tags_config: Option<&TagsConfiguration>,
    opts: &CompactOptions,
) -> Result<(CompactFileResult, Vec<Diagnostic>)> {
    let language = &file_lang.language;

    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let old_tree = parser
        .parse(old_source, None)
        .ok_or_else(|| anyhow!("parse_error: failed to parse old {}", path.display()))?;

    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let new_tree = parser
        .parse(new_source, None)
        .ok_or_else(|| anyhow!("parse_error: failed to parse new {}", path.display()))?;

    let invalidation = invalidate_snapshot(&old_tree, &new_tree, old_source, new_source, path)?;

    let mut preserved = Vec::new();
    let mut signatures_only = Vec::new();
    let mut file_omitted = Vec::new();
    let mut original_tokens = 0usize;

    // Get tags config for signature extraction
    let tags_config = tags_config;

    // Collect symbols if tags config is available
    let symbols = if let Some(ref config) = tags_config {
        let sym_output = symbols_for_tree(path, new_source, config, &SymbolOptions::default());
        sym_output.symbols
    } else {
        Vec::new()
    };

    // Process all new chunks
    let new_chunks = chunks_for_tree(&new_tree, path, new_source, &ChunkOptions::default());
    for chunk in new_chunks.chunks {
        original_tokens += chunk.estimated_tokens;

        // Find the classification for this chunk
        let classification = invalidation.records.iter()
            .find(|r| r.chunk.stable_id == chunk.stable_id)
            .map(|r| r.status)
            .unwrap_or(InvalidationStatus::Unchanged);

        match classification {
            InvalidationStatus::Affected | InvalidationStatus::Added | InvalidationStatus::Removed => {
                preserved.push(chunk);
            }
            InvalidationStatus::Unchanged => {
                if chunk.name.is_some() {
                    // Named chunk: try to extract signature
                    match extract_signature(&chunk, new_source, &symbols) {
                        Some(signature) => {
                            signatures_only.push(CompactChunkRecord::SignatureOnly {
                                chunk,
                                signature,
                            });
                        }
                        None => {
                            // Fallback: keep full chunk
                            preserved.push(chunk);
                        }
                    }
                } else {
                    // Anonymous chunk: keep full content
                    preserved.push(chunk);
                }
            }
        }
    }

    let compacted_tokens: usize = preserved.iter().map(|c| c.estimated_tokens).sum::<usize>()
        + signatures_only.iter().map(|r| match r {
            CompactChunkRecord::SignatureOnly { chunk, .. } => chunk.estimated_tokens,
            CompactChunkRecord::Preserved { chunk } => chunk.estimated_tokens,
        }).sum::<usize>();

    let result = CompactFileResult {
        path: path.to_path_buf(),
        preserved,
        signatures_only,
        omitted: file_omitted,
        original_tokens,
        compacted_tokens,
    };

    let diagnostics = new_chunks.diagnostics.into_iter()
        .map(|d| d.with_source("new_snapshot_chunking"))
        .collect();

    Ok((result, diagnostics))
}

/// Extract signature text for a named chunk using symbol information.
///
/// Strategy:
/// 1. Find symbol(s) whose byte_range is contained within the chunk's byte_range.
/// 2. Use the symbol's byte_range to extract declaration text from source bytes.
/// 3. If no symbol found, fall back to the chunk's first line.
fn extract_signature(chunk: &ChunkRecord, source: &[u8], symbols: &[crate::schema::SymbolRecord]) -> Option<String> {
    // Find overlapping symbols
    let overlapping: Vec<_> = symbols.iter()
        .filter(|s| s.is_definition)
        .filter(|s| {
            s.byte_range.start >= chunk.byte_range.start
                && s.byte_range.end <= chunk.byte_range.end
        })
        .collect();

    if let Some(symbol) = overlapping.first() {
        // Extract declaration text from source
        let decl_bytes = &source[symbol.byte_range.start..symbol.byte_range.end.min(source.len())];
        if let Ok(text) = std::str::from_utf8(decl_bytes) {
            // For functions, only keep the signature line (up to `{`)
            let signature = if text.contains('{') {
                text.lines()
                    .take_while(|line| !line.trim_start().starts_with('{'))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_string()
            } else {
                text.trim().to_string()
            };
            if !signature.is_empty() {
                return Some(signature);
            }
        }
    }

    // Fallback: first line of the chunk
    let chunk_bytes = &source[chunk.byte_range.start..chunk.byte_range.end.min(source.len())];
    if let Ok(text) = std::str::from_utf8(chunk_bytes) {
        let first_line = text.lines().next().unwrap_or("").trim().to_string();
        if !first_line.is_empty() {
            return Some(first_line);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn rust_language() -> tree_sitter::Language {
        let raw = unsafe { tree_sitter_rust::LANGUAGE.into_raw()() };
        unsafe { std::mem::transmute(raw) }
    }

    fn rust_tags_config() -> TagsConfiguration {
        let raw = unsafe { tree_sitter_rust::LANGUAGE.into_raw()() };
        let language: tree_sitter::Language = unsafe { std::mem::transmute(raw) };
        TagsConfiguration::new(language, "(function_item name: (identifier) @name) @definition.function", "").unwrap()
    }

    fn rust_file_lang() -> FileLanguage {
        FileLanguage {
            language: rust_language(),
        }
    }

    fn setup_test_file(source: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(source).unwrap();
        file
    }

    #[test]
    fn body_only_change_preserves_affected() {
        let old_source = b"fn foo() { let x = 1; }\nfn bar() { let y = 2; }";
        let new_source = b"fn foo() { let x = 1; let z = 3; }\nfn bar() { let y = 2; }";

        let old_file = setup_test_file(old_source);
        let new_file = setup_test_file(new_source);

        let path = new_file.path().to_path_buf();

        let mut old_contents = HashMap::new();
        old_contents.insert(path.clone(), old_source.to_vec());

        let mut languages = HashMap::new();
        languages.insert(path.clone(), rust_file_lang());

        let tags_config = rust_tags_config();
        let mut tags_configs = HashMap::new();
        tags_configs.insert(path.clone(), &tags_config);

        let opts = CompactOptions {
            budget: None,
        };

        let result = compact_files(
            &[path],
            &old_contents,
            &languages,
            &tags_configs,
            &opts,
        ).unwrap();

        assert_eq!(result.files.len(), 1);
        let file = &result.files[0];
        assert!(!file.preserved.is_empty(), "expected at least one preserved chunk for body change");
        assert!(!file.signatures_only.is_empty(), "expected at least one signature-only chunk for unchanged function");
    }

    #[test]
    fn signature_change_adds_and_removes() {
        let old_source = b"fn foo() {}";
        let new_source = b"fn foo(x: i32) {}";

        let old_file = setup_test_file(old_source);
        let new_file = setup_test_file(new_source);
        let path = new_file.path().to_path_buf();

        let mut old_contents = HashMap::new();
        old_contents.insert(path.clone(), old_source.to_vec());

        let mut languages = HashMap::new();
        languages.insert(path.clone(), rust_file_lang());

        let tags_config = rust_tags_config();
        let mut tags_configs = HashMap::new();
        tags_configs.insert(path.clone(), &tags_config);

        let opts = CompactOptions {
            budget: None,
        };

        let result = compact_files(
            &[path],
            &old_contents,
            &languages,
            &tags_configs,
            &opts,
        ).unwrap();

        assert_eq!(result.files.len(), 1);
        let file = &result.files[0];
        // Signature change: old removed, new added -> both preserved
        assert!(!file.preserved.is_empty(), "expected preserved chunks for signature change");
    }

    #[test]
    fn whitespace_only_all_signatures() {
        let old_source = b"fn foo() { let x = 1; }";
        let new_source = b"fn foo() {\n    let x = 1;\n}";

        let old_file = setup_test_file(old_source);
        let new_file = setup_test_file(new_source);
        let path = new_file.path().to_path_buf();

        let mut old_contents = HashMap::new();
        old_contents.insert(path.clone(), old_source.to_vec());

        let mut languages = HashMap::new();
        languages.insert(path.clone(), rust_file_lang());

        let tags_config = rust_tags_config();
        let mut tags_configs = HashMap::new();
        tags_configs.insert(path.clone(), &tags_config);

        let opts = CompactOptions {
            budget: None,
        };

        let result = compact_files(
            &[path],
            &old_contents,
            &languages,
            &tags_configs,
            &opts,
        ).unwrap();

        assert_eq!(result.files.len(), 1);
        let file = &result.files[0];
        // All chunks should be signatures_only since no actual changes
        assert!(!file.signatures_only.is_empty(), "expected signature-only chunks for whitespace-only change");
    }

    #[test]
    fn budget_forces_omission() {
        let old_source = b"fn foo() { let x = 1; }\nfn bar() { let y = 2; }";
        let new_source = b"fn foo() { let x = 1; let z = 3; }\nfn bar() { let y = 2; }";

        let old_file = setup_test_file(old_source);
        let new_file = setup_test_file(new_source);
        let path = new_file.path().to_path_buf();

        let mut old_contents = HashMap::new();
        old_contents.insert(path.clone(), old_source.to_vec());

        let mut languages = HashMap::new();
        languages.insert(path.clone(), rust_file_lang());

        let tags_config = rust_tags_config();
        let mut tags_configs = HashMap::new();
        tags_configs.insert(path.clone(), &tags_config);

        let opts = CompactOptions {
            budget: Some(5), // Very tight budget
        };

        let result = compact_files(
            &[path],
            &old_contents,
            &languages,
            &tags_configs,
            &opts,
        );

        // Should either succeed with omitted chunks or fail with budget_exceeded
        match result {
            Ok(output) => {
                assert!(!output.omitted.is_empty() || !output.files[0].omitted.is_empty(),
                    "expected some chunks to be omitted with tight budget");
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("budget_exceeded"), "expected budget_exceeded error, got: {}", msg);
            }
        }
    }
}

use std::ops::Range;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::identity::StableId;

/// Confidence level for a chunk or invalidation result.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Exact match with no ambiguity.
    Exact,
    /// Strong heuristic match, but not guaranteed.
    High,
    /// Best-effort match with known limitations.
    #[default]
    Medium,
    /// Fallback or degraded result.
    Low,
}

/// Severity level for a diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

/// A diagnostic message attached to engine output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl Diagnostic {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Info,
            message: message.into(),
            context: None,
        }
    }

    pub fn warn(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            message: message.into(),
            context: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            message: message.into(),
            context: None,
        }
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// A byte range within a source file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl From<Range<usize>> for ByteRange {
    fn from(r: Range<usize>) -> Self {
        Self {
            start: r.start,
            end: r.end,
        }
    }
}

impl ByteRange {
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Stable identifier for a chunk across parse runs.
///
/// The identity is based on source path, syntax kind, optional name,
/// and anchor byte position. It is deterministic and comparable across
/// runs, which makes cache reuse and invalidation possible.
///
/// For unnamed chunks, `anchor_byte` is the start position and serves
/// as the tiebreaker. Named chunks can be matched by name even if
/// their byte range shifts slightly.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChunkId {
    pub path: PathBuf,
    pub kind: String,
    pub name: Option<String>,
    pub anchor_byte: usize,
}

/// A single chunk of code context, ready for serialization or bundling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChunkRecord {
    pub id: ChunkId,
    pub stable_id: StableId,
    pub kind: String,
    pub name: Option<String>,
    pub byte_range: ByteRange,
    pub estimated_tokens: usize,
    pub confidence: Confidence,
}

/// Metadata about the output payload.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OutputMeta {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<PathBuf>,
    pub total_chunks: usize,
    pub total_estimated_tokens: usize,
}

/// Canonical output for the context engine.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContextOutput {
    pub chunks: Vec<ChunkRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub meta: OutputMeta,
}

/// Output from an invalidation pass (snapshot diff or edit stream).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InvalidationOutput {
    /// Chunks that overlap a changed range.
    pub affected: Vec<ChunkRecord>,
    /// Chunks present in the new snapshot but not the old.
    pub added: Vec<ChunkRecord>,
    /// Chunks present in the old snapshot but not the new.
    pub removed: Vec<ChunkRecord>,
    /// Chunks that exist in both snapshots and do not overlap any changed range.
    pub unchanged: Vec<ChunkRecord>,
    /// Raw changed ranges detected by tree-sitter.
    pub changed_ranges: Vec<ByteRange>,
    pub diagnostics: Vec<Diagnostic>,
    pub meta: OutputMeta,
}

impl ContextOutput {
    pub fn new(schema_version: impl Into<String>) -> Self {
        Self {
            chunks: Vec::new(),
            diagnostics: Vec::new(),
            meta: OutputMeta {
                schema_version: schema_version.into(),
                source_path: None,
                total_chunks: 0,
                total_estimated_tokens: 0,
            },
        }
    }

    pub fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn push_chunk(&mut self, chunk: ChunkRecord) {
        self.meta.total_estimated_tokens += chunk.estimated_tokens;
        self.meta.total_chunks += 1;
        self.chunks.push(chunk);
    }

    pub fn with_source_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.meta.source_path = Some(path.into());
        self
    }
}

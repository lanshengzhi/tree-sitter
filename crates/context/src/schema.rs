use std::ops::Range;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::identity::StableId;

/// Confidence level for a chunk or invalidation result.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

/// Stable machine-readable diagnostic code.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    GeneralInfo,
    GeneralWarning,
    GeneralError,
}

/// A diagnostic message attached to engine output.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub code: DiagnosticCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Info,
            code: DiagnosticCode::GeneralInfo,
            message: message.into(),
            cause: None,
            fix: None,
            context: None,
            source: None,
        }
    }

    #[must_use]
    pub fn warn(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            code: DiagnosticCode::GeneralWarning,
            message: message.into(),
            cause: None,
            fix: None,
            context: None,
            source: None,
        }
    }

    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            code: DiagnosticCode::GeneralError,
            message: message.into(),
            cause: None,
            fix: None,
            context: None,
            source: None,
        }
    }

    #[must_use]
    pub const fn with_code(mut self, code: DiagnosticCode) -> Self {
        self.code = code;
        self
    }

    #[must_use]
    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }

    #[must_use]
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

/// A byte range within a source file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
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
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
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
    /// Nesting depth in the syntax tree (0 = top-level declaration).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub depth: usize,
    /// Parent chunk identifier, if this chunk is nested inside another chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ChunkId>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(n: &usize) -> bool {
    *n == 0
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

/// A symbol (definition or reference) extracted from a source file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SymbolRecord {
    pub name: String,
    pub syntax_type: String,
    pub byte_range: ByteRange,
    /// Line range (`start_line..end_line`) of the symbol in the source file.
    pub lines: ByteRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    pub is_definition: bool,
    pub path: PathBuf,
    pub confidence: Confidence,
}

/// Classification assigned to a chunk during invalidation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvalidationStatus {
    Affected,
    Added,
    Removed,
    Unchanged,
}

/// Evidence used to match or classify a chunk during invalidation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchStrategy {
    StableId,
    ContentComparison,
    TextualRangeOverlap,
    EditRangeOverlap,
    Unmatched,
}

/// Machine-readable reason for an invalidation classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvalidationReason {
    ChangedRangeOverlap,
    ContentChanged,
    AddedChunk,
    RemovedChunk,
    NoChangeDetected,
    DegradedMatching,
}

/// Detailed invalidation classification record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InvalidationRecord {
    pub status: InvalidationStatus,
    pub chunk: ChunkRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_chunk: Option<ChunkRecord>,
    pub reason: InvalidationReason,
    pub match_strategy: MatchStrategy,
    pub confidence: Confidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_ranges: Vec<ByteRange>,
}

/// Canonical output for the context engine.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContextOutput {
    pub chunks: Vec<ChunkRecord>,
    pub symbols: Vec<SymbolRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub meta: OutputMeta,
}

/// A compacted chunk record: either preserved (full content) or signature-only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum CompactChunkRecord {
    /// Full chunk preserved (affected, added, removed, or anonymous unchanged).
    Preserved {
        #[serde(flatten)]
        chunk: ChunkRecord,
    },
    /// Signature-only for unchanged named chunks.
    SignatureOnly {
        #[serde(flatten)]
        chunk: ChunkRecord,
        /// Extracted signature text (e.g., function declaration line).
        signature: String,
    },
}

/// Metadata about an omitted chunk during compaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CompactOmittedRecord {
    pub stable_id: StableId,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub reason: String,
    pub estimated_tokens: usize,
}

/// Result for a single file after compaction.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompactFileResult {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved: Vec<ChunkRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signatures_only: Vec<CompactChunkRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted: Vec<CompactOmittedRecord>,
    pub original_tokens: usize,
    pub compacted_tokens: usize,
}

/// Top-level output from semantic compaction.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompactOutput {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<CompactFileResult>,
    pub original_tokens: usize,
    pub compacted_tokens: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted: Vec<CompactOmittedRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    pub meta: OutputMeta,
}

/// Output from an invalidation pass (snapshot diff or edit stream).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InvalidationOutput {
    /// Detailed classification records with reason and matching metadata.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<InvalidationRecord>,
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
    #[must_use]
    pub fn new(schema_version: impl Into<String>) -> Self {
        Self {
            chunks: Vec::new(),
            symbols: Vec::new(),
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

    #[must_use]
    pub fn with_source_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.meta.source_path = Some(path.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::StableId;

    #[test]
    fn confidence_serializes_to_snake_case() {
        let json = serde_json::to_string(&Confidence::Exact).unwrap();
        assert_eq!(json, "\"exact\"");
        let json = serde_json::to_string(&Confidence::High).unwrap();
        assert_eq!(json, "\"high\"");
        let json = serde_json::to_string(&Confidence::Medium).unwrap();
        assert_eq!(json, "\"medium\"");
        let json = serde_json::to_string(&Confidence::Low).unwrap();
        assert_eq!(json, "\"low\"");
    }

    #[test]
    fn diagnostic_level_serializes_to_snake_case() {
        let json = serde_json::to_string(&DiagnosticLevel::Info).unwrap();
        assert_eq!(json, "\"info\"");
        let json = serde_json::to_string(&DiagnosticLevel::Warning).unwrap();
        assert_eq!(json, "\"warning\"");
        let json = serde_json::to_string(&DiagnosticLevel::Error).unwrap();
        assert_eq!(json, "\"error\"");
    }

    #[test]
    fn diagnostic_code_serializes_to_snake_case() {
        let json = serde_json::to_string(&DiagnosticCode::GeneralInfo).unwrap();
        assert_eq!(json, "\"general_info\"");
        let json = serde_json::to_string(&DiagnosticCode::GeneralWarning).unwrap();
        assert_eq!(json, "\"general_warning\"");
        let json = serde_json::to_string(&DiagnosticCode::GeneralError).unwrap();
        assert_eq!(json, "\"general_error\"");
    }

    #[test]
    fn context_output_snapshot_stability() {
        let mut output = ContextOutput::new("0.1.0").with_source_path("src/lib.rs");
        output.push_chunk(ChunkRecord {
            id: ChunkId {
                path: PathBuf::from("src/lib.rs"),
                kind: "function_item".to_string(),
                name: Some("foo".to_string()),
                anchor_byte: 0,
            },
            stable_id: StableId("named:test".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            byte_range: ByteRange { start: 0, end: 11 },
            estimated_tokens: 3,
            confidence: Confidence::Exact,
            depth: 0,
            parent: None,
        });
        output.push_diagnostic(Diagnostic::info("test diagnostic"));

        let json = serde_json::to_string_pretty(&output).unwrap();

        assert_eq!(
            json,
            r#"{
  "chunks": [
    {
      "id": {
        "path": "src/lib.rs",
        "kind": "function_item",
        "name": "foo",
        "anchor_byte": 0
      },
      "stable_id": "named:test",
      "kind": "function_item",
      "name": "foo",
      "byte_range": {
        "start": 0,
        "end": 11
      },
      "estimated_tokens": 3,
      "confidence": "exact"
    }
  ],
  "symbols": [],
  "diagnostics": [
    {
      "level": "info",
      "code": "general_info",
      "message": "test diagnostic"
    }
  ],
  "meta": {
    "schema_version": "0.1.0",
    "source_path": "src/lib.rs",
    "total_chunks": 1,
    "total_estimated_tokens": 3
  }
}"#
        );

        // Round-trip check.
        let deserialized: ContextOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.meta.schema_version, "0.1.0");
        assert_eq!(deserialized.chunks.len(), 1);
        assert_eq!(deserialized.diagnostics.len(), 1);
    }

    #[test]
    fn invalidation_output_snapshot_stability() {
        let chunk = ChunkRecord {
            id: ChunkId {
                path: PathBuf::from("src/lib.rs"),
                kind: "function_item".to_string(),
                name: Some("foo".to_string()),
                anchor_byte: 0,
            },
            stable_id: StableId("named:test".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            byte_range: ByteRange { start: 0, end: 11 },
            estimated_tokens: 3,
            confidence: Confidence::Medium,
            depth: 0,
            parent: None,
        };
        let output = InvalidationOutput {
            records: vec![InvalidationRecord {
                status: InvalidationStatus::Affected,
                chunk: chunk.clone(),
                old_chunk: None,
                reason: InvalidationReason::ChangedRangeOverlap,
                match_strategy: MatchStrategy::TextualRangeOverlap,
                confidence: Confidence::Medium,
                changed_ranges: vec![ByteRange { start: 0, end: 11 }],
            }],
            affected: vec![chunk],
            added: vec![],
            removed: vec![],
            unchanged: vec![],
            changed_ranges: vec![ByteRange { start: 0, end: 11 }],
            diagnostics: vec![Diagnostic::info("1 affected chunk(s)")],
            meta: OutputMeta {
                schema_version: "0.1.0".to_string(),
                source_path: Some(PathBuf::from("src/lib.rs")),
                total_chunks: 1,
                total_estimated_tokens: 3,
            },
        };

        let json = serde_json::to_string_pretty(&output).unwrap();

        assert_eq!(
            json,
            r#"{
  "records": [
    {
      "status": "affected",
      "chunk": {
        "id": {
          "path": "src/lib.rs",
          "kind": "function_item",
          "name": "foo",
          "anchor_byte": 0
        },
        "stable_id": "named:test",
        "kind": "function_item",
        "name": "foo",
        "byte_range": {
          "start": 0,
          "end": 11
        },
        "estimated_tokens": 3,
        "confidence": "medium"
      },
      "reason": "changed_range_overlap",
      "match_strategy": "textual_range_overlap",
      "confidence": "medium",
      "changed_ranges": [
        {
          "start": 0,
          "end": 11
        }
      ]
    }
  ],
  "affected": [
    {
      "id": {
        "path": "src/lib.rs",
        "kind": "function_item",
        "name": "foo",
        "anchor_byte": 0
      },
      "stable_id": "named:test",
      "kind": "function_item",
      "name": "foo",
      "byte_range": {
        "start": 0,
        "end": 11
      },
      "estimated_tokens": 3,
      "confidence": "medium"
    }
  ],
  "added": [],
  "removed": [],
  "unchanged": [],
  "changed_ranges": [
    {
      "start": 0,
      "end": 11
    }
  ],
  "diagnostics": [
    {
      "level": "info",
      "code": "general_info",
      "message": "1 affected chunk(s)"
    }
  ],
  "meta": {
    "schema_version": "0.1.0",
    "source_path": "src/lib.rs",
    "total_chunks": 1,
    "total_estimated_tokens": 3
  }
}"#
        );

        let deserialized: InvalidationOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.records.len(), 1);
        assert_eq!(deserialized.affected.len(), 1);
        assert_eq!(deserialized.changed_ranges.len(), 1);
    }

    #[test]
    fn symbol_record_snapshot_stability() {
        let symbol = SymbolRecord {
            name: "foo".to_string(),
            syntax_type: "function".to_string(),
            byte_range: ByteRange { start: 0, end: 11 },
            lines: ByteRange { start: 0, end: 1 },
            docs: Some("Example docs".to_string()),
            is_definition: true,
            path: PathBuf::from("src/lib.rs"),
            confidence: Confidence::Exact,
        };

        let json = serde_json::to_string_pretty(&symbol).unwrap();

        assert_eq!(
            json,
            r#"{
  "name": "foo",
  "syntax_type": "function",
  "byte_range": {
    "start": 0,
    "end": 11
  },
  "lines": {
    "start": 0,
    "end": 1
  },
  "docs": "Example docs",
  "is_definition": true,
  "path": "src/lib.rs",
  "confidence": "exact"
}"#
        );

        let deserialized: SymbolRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "foo");
        assert!(deserialized.is_definition);
    }
}

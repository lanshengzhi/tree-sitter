use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Confidence level for a result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Confidence {
    Exact,
    High,
    Medium,
    Low,
}

impl Confidence {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Self::Medium
    }
}

/// Provenance metadata attached to every result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Provenance {
    pub strategy: String,
    pub confidence: Confidence,
    pub graph_snapshot_id: String,
    pub orientation_freshness: String,
}

impl Default for Provenance {
    fn default() -> Self {
        Self {
            strategy: "unknown".to_string(),
            confidence: Confidence::Medium,
            graph_snapshot_id: "unknown".to_string(),
            orientation_freshness: "unknown".to_string(),
        }
    }
}

impl Provenance {
    #[must_use]
    pub fn new(strategy: impl Into<String>, confidence: Confidence) -> Self {
        Self {
            strategy: strategy.into(),
            confidence,
            graph_snapshot_id: "unknown".to_string(),
            orientation_freshness: "unknown".to_string(),
        }
    }

    #[must_use]
    pub fn with_graph_state(
        mut self,
        snapshot_id: impl Into<String>,
        freshness: impl Into<String>,
    ) -> Self {
        self.graph_snapshot_id = snapshot_id.into();
        self.orientation_freshness = freshness.into();
        self
    }
}

/// A single AST cell included in a bundle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AstCell {
    pub stable_id: String,
    pub kind: String,
    pub name: Option<String>,
    pub byte_range: (usize, usize),
    pub estimated_tokens: usize,
    pub confidence: Confidence,
}

/// A chunk that was omitted from the bundle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OmittedChunk {
    pub stable_id: String,
    pub reason: String,
}

/// Candidate for ambiguous stable ID resolution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Candidate {
    pub anchor_byte: usize,
    pub kind: String,
    pub name: Option<String>,
}

/// Successful bundle result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Bundle {
    pub version: u32,
    pub path: PathBuf,
    pub cells: Vec<AstCell>,
    pub omitted: Vec<OmittedChunk>,
    pub provenance: Provenance,
}

/// Not found result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NotFound {
    pub path: PathBuf,
    pub stable_id: String,
    pub reason: String,
    pub provenance: Provenance,
}

/// Ambiguous stable ID result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AmbiguousStableId {
    pub path: PathBuf,
    pub stable_id: String,
    pub candidates: Vec<Candidate>,
    pub reason: String,
    pub provenance: Provenance,
}

/// Exhausted budget result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Exhausted {
    pub path: PathBuf,
    pub stable_id: String,
    pub omitted: Vec<OmittedChunk>,
    pub provenance: Provenance,
}

/// Unknown cross-file reference result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UnknownCrossFile {
    pub path: PathBuf,
    pub stable_id: String,
    pub reason: String,
}

/// Union of all possible v1 results.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum BundleResult {
    Bundle(Bundle),
    NotFound(NotFound),
    AmbiguousStableId(AmbiguousStableId),
    Exhausted(Exhausted),
    UnknownCrossFile(UnknownCrossFile),
}

/// Unsupported tier result (typed error).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnsupportedTier {
    pub tier: String,
    pub supported: Vec<String>,
}

/// Unsupported format result (typed error).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnsupportedFormat {
    pub format: String,
    pub supported: Vec<String>,
}

/// Union of possible validation errors before processing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidationError {
    UnsupportedTier(UnsupportedTier),
    UnsupportedFormat(UnsupportedFormat),
    InvalidStableId { message: String },
    PathTraversal { message: String },
}

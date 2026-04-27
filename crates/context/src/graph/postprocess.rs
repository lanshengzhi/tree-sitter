//! Postprocess artifact I/O for R3 god-nodes.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::graph::snapshot::GraphError;

/// Compile-time schema version for postprocess artifacts.
pub const POSTPROCESS_SCHEMA_VERSION: &str = "r3-god-nodes-2026-04-26";

/// A single god-node with deterministic rank.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GodNode {
    pub rank: usize,
    pub stable_id: String,
    pub path: String,
}

/// On-disk postprocess artifact.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PostprocessArtifact {
    pub snapshot_id: String,
    pub schema_version: String,
    pub computed_at: u64,
    pub god_nodes: Vec<GodNode>,
}

/// Typed result of reading a postprocess artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostprocessStatus {
    /// Artifact present and valid.
    Present(Vec<GodNode>),
    /// No artifact found for this snapshot.
    Missing,
    /// Artifact exists but could not be parsed or validated.
    Corrupt(String),
    /// Artifact schema version does not match expected version.
    SchemaMismatch(String),
    /// Artifact snapshot_id does not match the requested snapshot.
    SnapshotMismatch,
}

/// Write a postprocess artifact atomically.
///
/// Uses temp file + fsync + rename to prevent half-written reads.
pub fn write_postprocess_artifact(
    store_root: &Path,
    snapshot_id: String,
    god_nodes: Vec<GodNode>,
) -> Result<(), GraphError> {
    let postprocess_dir = store_root.join("postprocess");
    fs::create_dir_all(&postprocess_dir).map_err(|e| GraphError::WriteFailure {
        path: postprocess_dir.clone(),
        reason: format!("create postprocess dir failed: {e}"),
    })?;

    let path = postprocess_dir.join(format!("{snapshot_id}.json"));
    let artifact = PostprocessArtifact {
        snapshot_id: snapshot_id.clone(),
        schema_version: POSTPROCESS_SCHEMA_VERSION.to_string(),
        computed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        god_nodes,
    };

    let json = serde_json::to_string_pretty(&artifact).map_err(|e| GraphError::WriteFailure {
        path: path.clone(),
        reason: format!("serialization failed: {e}"),
    })?;

    let temp_path = path.with_extension("tmp");
    let mut temp_file = fs::File::create(&temp_path).map_err(|e| GraphError::WriteFailure {
        path: temp_path.clone(),
        reason: e.to_string(),
    })?;

    temp_file
        .write_all(json.as_bytes())
        .map_err(|e| GraphError::WriteFailure {
            path: temp_path.clone(),
            reason: e.to_string(),
        })?;

    temp_file
        .sync_all()
        .map_err(|e| GraphError::WriteFailure {
            path: temp_path.clone(),
            reason: e.to_string(),
        })?;

    drop(temp_file);

    fs::rename(&temp_path, &path).map_err(|e| GraphError::WriteFailure {
        path: path.clone(),
        reason: format!("atomic rename failed: {e}"),
    })?;

    Ok(())
}

/// Read and validate a postprocess artifact.
///
/// Returns `PostprocessStatus` with detailed failure reasons.
pub fn read_postprocess_artifact(
    store_root: &Path,
    snapshot_id: &str,
) -> PostprocessStatus {
    let path = store_root.join("postprocess").join(format!("{snapshot_id}.json"));

    if !path.exists() {
        return PostprocessStatus::Missing;
    }

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            return PostprocessStatus::Corrupt(format!("read failed: {e}"));
        }
    };

    let artifact: PostprocessArtifact = match serde_json::from_slice(&bytes) {
        Ok(a) => a,
        Err(e) => {
            return PostprocessStatus::Corrupt(format!("parse failed: {e}"));
        }
    };

    if artifact.schema_version != POSTPROCESS_SCHEMA_VERSION {
        return PostprocessStatus::SchemaMismatch(artifact.schema_version);
    }

    if artifact.snapshot_id != snapshot_id {
        return PostprocessStatus::SnapshotMismatch;
    }

    // Validate rank continuity: 1..K
    for (i, node) in artifact.god_nodes.iter().enumerate() {
        if node.rank != i + 1 {
            return PostprocessStatus::Corrupt(format!(
                "non-continuous rank at index {i}: expected {}, got {}",
                i + 1,
                node.rank
            ));
        }
    }

    PostprocessStatus::Present(artifact.god_nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_god_nodes() -> Vec<GodNode> {
        vec![
            GodNode {
                rank: 1,
                stable_id: "named:foo".to_string(),
                path: "src/lib.rs".to_string(),
            },
            GodNode {
                rank: 2,
                stable_id: "named:bar".to_string(),
                path: "src/main.rs".to_string(),
            },
        ]
    }

    #[test]
    fn write_and_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store_root = tmp.path();
        let nodes = sample_god_nodes();

        write_postprocess_artifact(store_root, "snap-123".to_string(), nodes.clone()).unwrap();

        match read_postprocess_artifact(store_root, "snap-123") {
            PostprocessStatus::Present(read_nodes) => assert_eq!(read_nodes, nodes),
            other => panic!("expected Present, got {other:?}"),
        }
    }

    #[test]
    fn empty_god_nodes() {
        let tmp = TempDir::new().unwrap();
        let store_root = tmp.path();

        write_postprocess_artifact(store_root, "snap-empty".to_string(), vec![]).unwrap();

        match read_postprocess_artifact(store_root, "snap-empty") {
            PostprocessStatus::Present(nodes) => assert!(nodes.is_empty()),
            other => panic!("expected Present, got {other:?}"),
        }
    }

    #[test]
    fn missing_artifact() {
        let tmp = TempDir::new().unwrap();
        let store_root = tmp.path();

        match read_postprocess_artifact(store_root, "snap-missing") {
            PostprocessStatus::Missing => {}
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    #[test]
    fn corrupt_json() {
        let tmp = TempDir::new().unwrap();
        let store_root = tmp.path();
        let path = store_root.join("postprocess").join("snap-bad.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not json").unwrap();

        match read_postprocess_artifact(store_root, "snap-bad") {
            PostprocessStatus::Corrupt(_) => {}
            other => panic!("expected Corrupt, got {other:?}"),
        }
    }

    #[test]
    fn schema_mismatch() {
        let tmp = TempDir::new().unwrap();
        let store_root = tmp.path();
        let path = store_root.join("postprocess").join("snap-old.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        let artifact = PostprocessArtifact {
            snapshot_id: "snap-old".to_string(),
            schema_version: "old-version".to_string(),
            computed_at: 0,
            god_nodes: vec![],
        };
        fs::write(&path, serde_json::to_string_pretty(&artifact).unwrap()).unwrap();

        match read_postprocess_artifact(store_root, "snap-old") {
            PostprocessStatus::SchemaMismatch(v) => assert_eq!(v, "old-version"),
            other => panic!("expected SchemaMismatch, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_mismatch() {
        let tmp = TempDir::new().unwrap();
        let store_root = tmp.path();
        let path = store_root.join("postprocess").join("snap-wrong.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        let artifact = PostprocessArtifact {
            snapshot_id: "different-id".to_string(),
            schema_version: POSTPROCESS_SCHEMA_VERSION.to_string(),
            computed_at: 0,
            god_nodes: vec![],
        };
        fs::write(&path, serde_json::to_string_pretty(&artifact).unwrap()).unwrap();

        match read_postprocess_artifact(store_root, "snap-wrong") {
            PostprocessStatus::SnapshotMismatch => {}
            other => panic!("expected SnapshotMismatch, got {other:?}"),
        }
    }

    #[test]
    fn non_continuous_ranks() {
        let tmp = TempDir::new().unwrap();
        let store_root = tmp.path();
        let nodes = vec![
            GodNode {
                rank: 1,
                stable_id: "named:a".to_string(),
                path: "a.rs".to_string(),
            },
            GodNode {
                rank: 3, // skip 2
                stable_id: "named:b".to_string(),
                path: "b.rs".to_string(),
            },
        ];

        write_postprocess_artifact(store_root, "snap-ranks".to_string(), nodes).unwrap();

        match read_postprocess_artifact(store_root, "snap-ranks") {
            PostprocessStatus::Corrupt(_) => {}
            other => panic!("expected Corrupt, got {other:?}"),
        }
    }
}

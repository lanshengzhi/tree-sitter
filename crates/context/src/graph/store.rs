//! Graph snapshot store with atomic HEAD management.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::graph::snapshot::{
    canonicalize_snapshot, GraphError, GraphSnapshot, GraphSnapshotId, GRAPH_SCHEMA_VERSION,
};

/// Store directory name inside a repo.
pub const STORE_DIR_NAME: &str = ".tree-sitter-context-mcp";
/// HEAD file name.
pub const HEAD_FILE_NAME: &str = "HEAD";

/// Manages durable graph snapshots under a repo-local directory.
#[derive(Clone, Debug)]
pub struct GraphStore {
    root: PathBuf,
}

impl GraphStore {
    /// Open or create a graph store at `repo_root/.tree-sitter-context-mcp/`.
    pub fn open(repo_root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = repo_root.as_ref().join(STORE_DIR_NAME);
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Return the path to the store root.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Write a snapshot durably and return its ID.
    ///
    /// Uses a temp file + atomic rename. If the same canonical snapshot
    /// already exists, it is not rewritten.
    pub fn write_snapshot(&self,
        snapshot: GraphSnapshot,
    ) -> Result<GraphSnapshotId, GraphError> {
        let canonical = canonicalize_snapshot(snapshot);
        let id = canonical.snapshot_id.clone();
        let path = self.snapshot_path(&id);

        if path.exists() {
            return Ok(id);
        }

        let json = serde_json::to_string_pretty(&canonical)
            .map_err(|e| GraphError::WriteFailure {
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

        // fsync to ensure durability
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

        Ok(id)
    }

    /// Read a snapshot by ID.
    pub fn read_snapshot(&self,
        id: &GraphSnapshotId,
    ) -> Result<GraphSnapshot, GraphError> {
        let path = self.snapshot_path(id);
        if !path.exists() {
            return Err(GraphError::MissingSnapshot {
                snapshot_id: id.clone(),
            });
        }

        let bytes = fs::read(&path).map_err(|e| GraphError::CorruptedSnapshot {
            snapshot_id: id.clone(),
            reason: format!("read failed: {e}"),
        })?;

        let snapshot: GraphSnapshot =
            serde_json::from_slice(&bytes).map_err(|e| GraphError::CorruptedSnapshot {
                snapshot_id: id.clone(),
                reason: format!("parse failed: {e}"),
            })?;

        Ok(snapshot)
    }

    /// Atomically update HEAD to point at `snapshot_id`.
    ///
    /// Writes HEAD only after verifying the snapshot exists and is readable.
    pub fn update_head(&self,
        snapshot_id: &GraphSnapshotId,
    ) -> Result<(), GraphError> {
        // Verify snapshot exists before updating HEAD
        let _ = self.read_snapshot(snapshot_id)?;

        let head_path = self.head_path();
        let temp_path = head_path.with_extension("tmp");

        let mut temp_file = fs::File::create(&temp_path).map_err(|e| GraphError::WriteFailure {
            path: temp_path.clone(),
            reason: e.to_string(),
        })?;

        writeln!(temp_file, "{}", snapshot_id.0).map_err(|e| GraphError::WriteFailure {
            path: temp_path.clone(),
            reason: e.to_string(),
        })?;

        temp_file.sync_all().map_err(|e| GraphError::WriteFailure {
            path: temp_path.clone(),
            reason: e.to_string(),
        })?;

        drop(temp_file);

        fs::rename(&temp_path, &head_path).map_err(|e| GraphError::WriteFailure {
            path: head_path.clone(),
            reason: format!("atomic rename failed: {e}"),
        })?;

        Ok(())
    }

    /// Read the current HEAD snapshot ID, if any.
    pub fn read_head(&self) -> Result<GraphSnapshotId, GraphError> {
        let head_path = self.head_path();
        if !head_path.exists() {
            return Err(GraphError::MissingSnapshot {
                snapshot_id: GraphSnapshotId("HEAD".to_string()),
            });
        }

        let contents = fs::read_to_string(&head_path).map_err(|e| GraphError::CorruptedSnapshot {
            snapshot_id: GraphSnapshotId("HEAD".to_string()),
            reason: format!("HEAD read failed: {e}"),
        })?;

        let id = contents.trim();
        Ok(GraphSnapshotId(id.to_string()))
    }

    /// Verify store integrity.
    ///
    /// Checks HEAD presence, target readability, schema compatibility,
    /// and canonical hash match.
    pub fn verify(&self) -> Result<(), GraphError> {
        let head_id = self.read_head()?;
        let snapshot = self.read_snapshot(&head_id)?;

        if snapshot.schema_version != GRAPH_SCHEMA_VERSION {
            return Err(GraphError::SchemaMismatch {
                expected: GRAPH_SCHEMA_VERSION.to_string(),
                found: snapshot.schema_version,
            });
        }

        // Re-canonicalize and verify hash
        let recomputed = canonicalize_snapshot(snapshot);
        if recomputed.snapshot_id != head_id {
            return Err(GraphError::CorruptedSnapshot {
                snapshot_id: head_id,
                reason: "canonical hash mismatch".to_string(),
            });
        }

        Ok(())
    }

    /// Conservative garbage collection: remove unreachable snapshots.
    ///
    /// Only removes snapshots that are not the current HEAD target.
    /// Returns count of removed snapshots.
    pub fn clean(&self) -> Result<usize, GraphError> {
        let head_id = match self.read_head() {
            Ok(id) => id,
            Err(_) => return Ok(0), // No HEAD, nothing to preserve
        };
        let head_path = self.snapshot_path(&head_id);

        let entries = match fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(_) => return Ok(0),
        };

        let mut removed = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path == head_path {
                continue;
            }
            if path.extension().is_some_and(|ext| ext == "tmp") {
                // Remove stale temp files
                let _ = fs::remove_file(&path);
                removed += 1;
            } else if path.is_file() && path != self.head_path() {
                // Only remove snapshot files (JSON files with hex names)
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if name.len() == 32 && name.chars().all(|c| c.is_ascii_hexdigit()) {
                    let _ = fs::remove_file(&path);
                    removed += 1;
                }
            }
        }

        Ok(removed)
    }

    fn snapshot_path(&self, id: &GraphSnapshotId) -> PathBuf {
        self.root.join(format!("{}.json", id.0))
    }

    fn head_path(&self) -> PathBuf {
        self.root.join(HEAD_FILE_NAME)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;
    use crate::graph::snapshot::{
        GraphFile, GraphNode, GraphSnapshot, GraphSnapshotId, GRAPH_SCHEMA_VERSION,
    };
    use crate::{ByteRange, Confidence, StableId};

    fn test_snapshot() -> GraphSnapshot {
        let node = GraphNode {
            path: PathBuf::from("src/lib.rs"),
            stable_id: StableId("named:foo".to_string()),
            kind: "function_item".to_string(),
            name: Some("foo".to_string()),
            anchor_byte: 0,
            byte_range: ByteRange { start: 0, end: 10 },
            signature_hash: None,
            content_hash: None,
            confidence: Confidence::Exact,
        };

        let file = GraphFile {
            path: PathBuf::from("src/lib.rs"),
            content_hash: None,
            nodes: vec![node],
            symbols: vec![],
            diagnostics: vec![],
        };

        canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![file],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        })
    }

    #[test]
    fn ae1_write_and_read_snapshot() {
        let tmp = TempDir::new().unwrap();
        let store = GraphStore::open(tmp.path()).unwrap();

        let snapshot = test_snapshot();
        let id = store.write_snapshot(snapshot.clone()).unwrap();

        let read = store.read_snapshot(&id).unwrap();
        assert_eq!(read.snapshot_id, id);
        assert_eq!(read.files.len(), 1);
    }

    #[test]
    fn ae2_same_snapshot_same_id() {
        let tmp = TempDir::new().unwrap();
        let store = GraphStore::open(tmp.path()).unwrap();

        let snapshot = test_snapshot();
        let id1 = store.write_snapshot(snapshot.clone()).unwrap();
        let id2 = store.write_snapshot(snapshot).unwrap();

        assert_eq!(id1, id2, "same canonical snapshot must produce same id");
    }

    #[test]
    fn ae1_head_updated_after_write() {
        let tmp = TempDir::new().unwrap();
        let store = GraphStore::open(tmp.path()).unwrap();

        let snapshot = test_snapshot();
        let id = store.write_snapshot(snapshot).unwrap();
        store.update_head(&id).unwrap();

        let head = store.read_head().unwrap();
        assert_eq!(head, id);
    }

    #[test]
    fn ae5_corrupt_snapshot_rejected() {
        let tmp = TempDir::new().unwrap();
        let store = GraphStore::open(tmp.path()).unwrap();

        // Write a corrupt snapshot file directly
        let corrupt_id = GraphSnapshotId("0".repeat(32));
        let path = store.path().join(format!("{}.json", corrupt_id.0));
        fs::write(&path, b"not json").unwrap();

        let err = store.read_snapshot(&corrupt_id).unwrap_err();
        match err {
            GraphError::CorruptedSnapshot { .. } => {}
            other => panic!("expected corrupted snapshot error, got: {other}"),
        }
    }

    #[test]
    fn ae5_verify_catches_schema_mismatch() {
        let tmp = TempDir::new().unwrap();
        let store = GraphStore::open(tmp.path()).unwrap();

        let snapshot = canonicalize_snapshot(GraphSnapshot {
            schema_version: "old-version".to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });
        let id = store.write_snapshot(snapshot).unwrap();
        store.update_head(&id).unwrap();

        let err = store.verify().unwrap_err();
        match err {
            GraphError::SchemaMismatch { expected, found } => {
                assert_eq!(expected, GRAPH_SCHEMA_VERSION);
                assert_eq!(found, "old-version");
            }
            other => panic!("expected schema mismatch, got: {other}"),
        }
    }

    #[test]
    fn clean_preserves_head() {
        let tmp = TempDir::new().unwrap();
        let store = GraphStore::open(tmp.path()).unwrap();

        let snapshot = test_snapshot();
        let id = store.write_snapshot(snapshot).unwrap();
        store.update_head(&id).unwrap();

        // Write another snapshot
        let other = canonicalize_snapshot(GraphSnapshot {
            schema_version: GRAPH_SCHEMA_VERSION.to_string(),
            snapshot_id: GraphSnapshotId(String::new()),
            files: vec![],
            edges: vec![],
            diagnostics: vec![],
            meta: None,
        });
        let other_id = store.write_snapshot(other).unwrap();

        let removed = store.clean().unwrap();
        assert_eq!(removed, 1);

        // HEAD target still readable
        assert!(store.read_snapshot(&id).is_ok());
        // Other snapshot removed
        assert!(store.read_snapshot(&other_id).is_err());
    }

    #[test]
    fn interrupted_write_leaves_head_intact() {
        let tmp = TempDir::new().unwrap();
        let store = GraphStore::open(tmp.path()).unwrap();

        let snapshot = test_snapshot();
        let id = store.write_snapshot(snapshot).unwrap();
        store.update_head(&id).unwrap();

        // Simulate interrupted write by creating a temp file
        let temp_path = store.path().join("snapshot.tmp");
        fs::write(&temp_path, b"partial data").unwrap();

        // HEAD should still point to valid snapshot
        let head = store.read_head().unwrap();
        assert_eq!(head, id);
        assert!(store.verify().is_ok());

        // Clean should remove the temp file
        let removed = store.clean().unwrap();
        assert_eq!(removed, 1);
        assert!(!temp_path.exists());
    }
}

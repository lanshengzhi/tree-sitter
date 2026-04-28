//! Snapshot disk cache for AST outlines.
//!
//! Stores parsed file snapshots as JSON under `.tree-sitter-context-mcp/cache/`
//! so the CLI can retrieve old snapshots by ID during delta computation.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::schema::ChunkRecord;

/// Store subdirectory for snapshot cache.
pub const CACHE_DIR_NAME: &str = "cache";
/// Maximum number of snapshots to keep in cache.
pub const MAX_SNAPSHOTS: usize = 1000;

/// A cached snapshot with metadata.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CachedSnapshot {
    pub file_path: PathBuf,
    pub created_at: u64,
    pub chunks: Vec<ChunkRecord>,
}

/// Disk cache for AST snapshots.
#[derive(Clone, Debug)]
pub struct SnapshotCache {
    root: PathBuf,
}

impl SnapshotCache {
    /// Open or create the snapshot cache at `repo_root/.tree-sitter-context-mcp/cache/`.
    pub fn open(repo_root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = repo_root.as_ref().join(crate::graph::store::STORE_DIR_NAME).join(CACHE_DIR_NAME);
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Save a snapshot to disk cache.
    pub fn save(
        &self,
        snapshot_id: &str,
        path: &Path,
        chunks: &[ChunkRecord],
    ) -> std::io::Result<()> {
        let snapshot = CachedSnapshot {
            file_path: path.to_path_buf(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            chunks: chunks.to_vec(),
        };

        let json = serde_json::to_string_pretty(&snapshot)?;
        let file_path = self.snapshot_path(snapshot_id);
        let temp_path = file_path.with_extension("tmp");

        let mut temp_file = fs::File::create(&temp_path)?;
        temp_file.write_all(json.as_bytes())?;
        temp_file.sync_all()?;
        drop(temp_file);

        fs::rename(&temp_path, &file_path)?;

        self.evict_if_needed()?;

        Ok(())
    }

    /// Load a snapshot from disk cache.
    pub fn load(&self,
        snapshot_id: &str,
    ) -> std::io::Result<Option<CachedSnapshot>> {
        let path = self.snapshot_path(snapshot_id);
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(&path)?;
        let snapshot: CachedSnapshot = match serde_json::from_slice(&bytes) {
            Ok(s) => s,
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("corrupted snapshot cache file: {e}"),
                ));
            }
        };

        Ok(Some(snapshot))
    }

    /// Remove oldest snapshots if count exceeds MAX_SNAPSHOTS.
    fn evict_if_needed(&self,
    ) -> std::io::Result<()> {
        let mut entries: Vec<(fs::DirEntry, std::time::SystemTime)> = Vec::new();

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                let modified = metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                entries.push((entry, modified));
            }
        }

        if entries.len() <= MAX_SNAPSHOTS {
            return Ok(());
        }

        // Sort by modification time (oldest first)
        entries.sort_by(|a, b| a.1.cmp(&b.1));

        let to_remove = entries.len() - MAX_SNAPSHOTS;
        for (entry, _) in entries.into_iter().take(to_remove) {
            fs::remove_file(entry.path())?;
        }

        Ok(())
    }

    fn snapshot_path(&self, snapshot_id: &str) -> PathBuf {
        self.root.join(format!("{snapshot_id}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ByteRange, ChunkId, ChunkRecord, Confidence};
    use crate::identity::StableId;
    use std::path::PathBuf;

    fn dummy_chunks() -> Vec<ChunkRecord> {
        vec![ChunkRecord {
            id: ChunkId {
                path: PathBuf::from("test.rs"),
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
            signature_hash: "sig_hash".to_string(),
            body_hash: "body_hash".to_string(),
        }]
    }

    #[test]
    fn save_and_load_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SnapshotCache::open(temp_dir.path()).unwrap();

        let chunks = dummy_chunks();
        cache.save("snap_123", Path::new("test.rs"), &chunks).unwrap();

        let loaded = cache.load("snap_123").unwrap().unwrap();
        assert_eq!(loaded.file_path, Path::new("test.rs"));
        assert_eq!(loaded.chunks.len(), 1);
        assert_eq!(loaded.chunks[0].stable_id.0, "named:test");
    }

    #[test]
    fn load_nonexistent_returns_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SnapshotCache::open(temp_dir.path()).unwrap();

        let result = cache.load("snap_missing").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn eviction_removes_oldest_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SnapshotCache::open(temp_dir.path()).unwrap();

        // Save MAX_SNAPSHOTS + 5 snapshots
        for i in 0..(MAX_SNAPSHOTS + 5) {
            cache.save(&format!("snap_{i:04}"), Path::new("test.rs"), &dummy_chunks()).unwrap();
            // Small delay to ensure different modification times
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let entries: Vec<_> = fs::read_dir(&cache.root).unwrap().collect();
        assert_eq!(entries.len(), MAX_SNAPSHOTS);

        // Oldest files should be evicted
        for i in 0..5 {
            assert!(cache.load(&format!("snap_{i:04}")).unwrap().is_none());
        }

        // Newer files should remain
        for i in (MAX_SNAPSHOTS)..(MAX_SNAPSHOTS + 5) {
            assert!(cache.load(&format!("snap_{i:04}")).unwrap().is_some());
        }
    }

    #[test]
    fn corrupted_json_returns_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SnapshotCache::open(temp_dir.path()).unwrap();

        // Write invalid JSON directly
        let path = cache.snapshot_path("snap_bad");
        fs::write(&path, b"not json").unwrap();

        let result = cache.load("snap_bad");
        assert!(result.is_err());
    }
}

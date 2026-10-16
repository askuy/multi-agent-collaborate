use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use trellis_core::{BlobRef, Hash};

use crate::error::{Result, StoreError};

/// Immutable, content-addressed byte storage.
pub trait BlobStore: Send + Sync {
    /// Stores `data` (idempotent) and returns its reference.
    fn put(&self, data: &[u8]) -> Result<BlobRef>;
    /// Returns the bytes for `hash`, verified against it, or `None` if absent.
    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>>;
    fn contains(&self, hash: &Hash) -> Result<bool>;
}

/// Blobs as zstd-compressed files at `root/<first 2 hex>/<remaining 62 hex>`.
pub struct FsBlobStore {
    root: PathBuf,
}

/// Blobs in a hash map; for tests and simulation.
#[derive(Default)]
pub struct MemBlobStore {
    blobs: Mutex<HashMap<Hash, Vec<u8>>>,
}

impl FsBlobStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// The fan-out directory and file path for `hash`.
    fn locate(&self, hash: &Hash) -> (PathBuf, PathBuf) {
        let hex = hash.to_hex();
        let dir = self.root.join(&hex[..2]);
        let path = dir.join(&hex[2..]);
        (dir, path)
    }
}

impl BlobStore for FsBlobStore {
    fn put(&self, data: &[u8]) -> Result<BlobRef> {
        let blob = BlobRef::of(data);
        let (dir, path) = self.locate(&blob.hash);
        if path.exists() {
            return Ok(blob);
        }
        std::fs::create_dir_all(&dir)?;
        let compressed = zstd::encode_all(data, 3)?;
        let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
        tmp.write_all(&compressed)?;
        tmp.as_file().sync_all()?;
        tmp.persist(&path).map_err(|e| StoreError::Io(e.error))?;
        Ok(blob)
    }

    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        let (_, path) = self.locate(hash);
        let compressed = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let data =
            zstd::decode_all(compressed.as_slice()).map_err(|_| StoreError::Corrupt(*hash))?;
        if Hash::of(&data) != *hash {
            return Err(StoreError::Corrupt(*hash));
        }
        Ok(Some(data))
    }

    fn contains(&self, hash: &Hash) -> Result<bool> {
        Ok(self.locate(hash).1.exists())
    }
}

impl MemBlobStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BlobStore for MemBlobStore {
    fn put(&self, data: &[u8]) -> Result<BlobRef> {
        let blob = BlobRef::of(data);
        self.blobs
            .lock()
            .map_err(|_| StoreError::Poisoned)?
            .entry(blob.hash)
            .or_insert_with(|| data.to_vec());
        Ok(blob)
    }

    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        Ok(self
            .blobs
            .lock()
            .map_err(|_| StoreError::Poisoned)?
            .get(hash)
            .cloned())
    }

    fn contains(&self, hash: &Hash) -> Result<bool> {
        Ok(self
            .blobs
            .lock()
            .map_err(|_| StoreError::Poisoned)?
            .contains_key(hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(store: &dyn BlobStore) {
        let a = store.put(b"hello").unwrap();
        assert_eq!(a, BlobRef::of(b"hello"));
        assert_eq!(store.put(b"hello").unwrap(), a);
        assert_eq!(store.get(&a.hash).unwrap().unwrap(), b"hello");
        assert!(store.contains(&a.hash).unwrap());

        let missing = Hash::of(b"missing");
        assert_eq!(store.get(&missing).unwrap(), None);
        assert!(!store.contains(&missing).unwrap());

        let empty = store.put(b"").unwrap();
        assert_eq!(store.get(&empty.hash).unwrap().unwrap(), b"");
    }

    #[test]
    fn fs_store_behaves() {
        let dir = tempfile::tempdir().unwrap();
        exercise(&FsBlobStore::open(dir.path()).unwrap());
    }

    #[test]
    fn mem_store_behaves() {
        exercise(&MemBlobStore::new());
    }

    #[test]
    fn fs_store_detects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path()).unwrap();
        let blob = store.put(b"good").unwrap();
        let (_, path) = store.locate(&blob.hash);
        std::fs::write(&path, zstd::encode_all(&b"evil"[..], 3).unwrap()).unwrap();
        assert!(matches!(store.get(&blob.hash), Err(StoreError::Corrupt(h)) if h == blob.hash));
    }

    #[test]
    fn fs_store_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let blob = FsBlobStore::open(dir.path())
            .unwrap()
            .put(b"durable")
            .unwrap();
        let reopened = FsBlobStore::open(dir.path()).unwrap();
        assert_eq!(reopened.get(&blob.hash).unwrap().unwrap(), b"durable");
    }
}

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use trellis_core::{Changeset, FileChange, FileKind, Hash, Provenance, RepoPath};
use trellis_store::{BlobStore, MemBlobStore, MetaStore, SqliteMetaStore};

use crate::framework::DeriveCtx;

pub(crate) const REPO: &str = "repo";

/// An in-memory repository for derive tests.
pub(crate) struct Fixture {
    pub meta: SqliteMetaStore,
    pub blobs: MemBlobStore,
    /// Makes every `chain` call write distinct contents, so independent chains never share
    /// changeset ids.
    next_chain: AtomicUsize,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            meta: SqliteMetaStore::open_in_memory().unwrap(),
            blobs: MemBlobStore::new(),
            next_chain: AtomicUsize::new(0),
        }
    }

    pub fn ctx(&self) -> DeriveCtx<'_> {
        DeriveCtx::new(REPO, &self.meta, &self.blobs)
    }

    pub fn meta_get_derived(&self, id: &Hash, name: &str, version: u32) -> Option<Vec<u8>> {
        self.meta.get_derived(REPO, id, name, version).unwrap()
    }

    /// Stores a changeset on top of `parent`; each edit is `(path, Some(content))` or
    /// `(path, None)` for a delete.
    pub fn commit(&self, parent: Option<Hash>, edits: &[(&str, Option<&str>)]) -> Hash {
        let mut changes = BTreeMap::new();
        for (path, content) in edits {
            let path = RepoPath::parse(path).unwrap();
            let change = match content {
                Some(text) => FileChange::Upsert {
                    blob: self.blobs.put(text.as_bytes()).unwrap(),
                    kind: FileKind::infer(&path, text.as_bytes()),
                },
                None => FileChange::Delete,
            };
            changes.insert(path, change);
        }
        let cs = Changeset {
            parents: parent.into_iter().collect(),
            changes,
            provenance: Provenance {
                principal: "test".into(),
                ..Default::default()
            },
            message: String::new(),
            created_at_ms: 0,
        };
        self.meta.put_changeset(REPO, &cs).unwrap()
    }

    /// A linear chain of `n` changesets on top of `parent`, each rewriting `f.txt` with
    /// contents unique to this chain.
    pub fn chain(&self, parent: Option<Hash>, n: usize) -> Vec<Hash> {
        let chain = self.next_chain.fetch_add(1, Ordering::Relaxed);
        let mut ids = Vec::with_capacity(n);
        let mut tip = parent;
        for i in 0..n {
            let content = format!("chain {chain} step {i}");
            let id = self.commit(tip, &[("f.txt", Some(content.as_str()))]);
            ids.push(id);
            tip = Some(id);
        }
        ids
    }
}

use trellis_core::{Changeset, Hash};

use crate::error::Result;

/// A ref's current target and how many times it has moved.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RefState {
    pub target: Hash,
    pub version: u64,
}

/// Changesets, refs, and derived data for many repositories.
pub trait MetaStore: Send + Sync {
    /// Stores a changeset (idempotent) and returns its id.
    fn put_changeset(&self, repo: &str, cs: &Changeset) -> Result<Hash>;
    fn get_changeset(&self, repo: &str, id: &Hash) -> Result<Option<Changeset>>;
    fn get_ref(&self, repo: &str, name: &str) -> Result<Option<RefState>>;
    /// Atomically moves `name` from `expected` to `new`; `expected = None` means the ref must
    /// not exist yet. Returns `false`, changing nothing, if the current state differs.
    fn cas_ref(&self, repo: &str, name: &str, expected: Option<Hash>, new: Hash) -> Result<bool>;
    /// Stores a derived value; the first write for a key wins (values are deterministic).
    fn put_derived(
        &self,
        repo: &str,
        cs: &Hash,
        name: &str,
        version: u32,
        value: &[u8],
    ) -> Result<()>;
    fn get_derived(
        &self,
        repo: &str,
        cs: &Hash,
        name: &str,
        version: u32,
    ) -> Result<Option<Vec<u8>>>;
    /// Upserts an opaque record (sessions, receipts, conflicts, ...) keyed by `(kind, id)`.
    fn put_record(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<()>;
    /// Inserts a record only if absent; returns whether it was inserted.
    fn put_record_if_absent(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<bool>;
    fn get_record(&self, repo: &str, kind: &str, id: &str) -> Result<Option<Vec<u8>>>;
    /// Every record of `kind`, ordered by id.
    fn list_records(&self, repo: &str, kind: &str) -> Result<Vec<(String, Vec<u8>)>>;
    fn delete_record(&self, repo: &str, kind: &str, id: &str) -> Result<()>;
}

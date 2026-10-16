use std::collections::HashSet;

use trellis_core::{Changeset, Hash};
use trellis_store::{BlobStore, MetaStore};

use crate::error::{DeriveError, Result};

/// A value computed from a changeset and, recursively, from its parents' values.
pub trait Derivable: Sized {
    /// Storage name; with `VERSION`, part of the cache key.
    const NAME: &'static str;
    /// Bump to re-derive with a new format; old rows stay until pruned.
    const VERSION: u32;
    /// Computes the value for `cs`. Every parent's value is already stored when this runs,
    /// so `ctx.get::<Self>(parent)` is a cache hit.
    fn derive(ctx: &DeriveCtx<'_>, id: &Hash, cs: &Changeset) -> Result<Self>;
    fn encode(&self) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> Result<Self>;
}

/// Access to one repository's changesets, blobs, and derived values.
pub struct DeriveCtx<'a> {
    pub repo: &'a str,
    pub meta: &'a dyn MetaStore,
    pub blobs: &'a dyn BlobStore,
}

impl<'a> DeriveCtx<'a> {
    pub fn new(repo: &'a str, meta: &'a dyn MetaStore, blobs: &'a dyn BlobStore) -> Self {
        Self { repo, meta, blobs }
    }

    pub fn changeset(&self, id: &Hash) -> Result<Changeset> {
        self.meta
            .get_changeset(self.repo, id)?
            .ok_or(DeriveError::MissingChangeset(*id))
    }

    fn stored<D: Derivable>(&self, id: &Hash) -> Result<Option<Vec<u8>>> {
        Ok(self.meta.get_derived(self.repo, id, D::NAME, D::VERSION)?)
    }

    /// Returns `D` for `id`, first deriving it for every ancestor that lacks it, parents
    /// before children, with an explicit stack (no recursion, so long histories are fine).
    pub fn get<D: Derivable>(&self, id: &Hash) -> Result<D> {
        if let Some(bytes) = self.stored::<D>(id)? {
            return D::decode(&bytes);
        }
        let mut stack: Vec<(Hash, bool)> = vec![(*id, false)];
        let mut done: HashSet<Hash> = HashSet::new();
        while let Some((current, parents_ready)) = stack.pop() {
            if done.contains(&current) {
                continue;
            }
            if self.stored::<D>(&current)?.is_some() {
                done.insert(current);
                continue;
            }
            let cs = self.changeset(&current)?;
            if parents_ready {
                let value = D::derive(self, &current, &cs)?;
                self.meta
                    .put_derived(self.repo, &current, D::NAME, D::VERSION, &value.encode())?;
                done.insert(current);
            } else {
                stack.push((current, true));
                for parent in &cs.parents {
                    if !done.contains(parent) {
                        stack.push((*parent, false));
                    }
                }
            }
        }
        let bytes = self
            .stored::<D>(id)?
            .ok_or(DeriveError::MissingChangeset(*id))?;
        D::decode(&bytes)
    }
}

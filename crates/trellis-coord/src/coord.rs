use std::sync::Arc;

use serde::Serialize;
use serde::de::DeserializeOwned;
use trellis_store::MetaStore;

use crate::env::Env;
use crate::error::Result;
use crate::locks::KeyedLocks;

/// Coordination state for one repository.
pub struct Coord {
    pub(crate) repo: String,
    pub(crate) meta: Arc<dyn MetaStore>,
    pub(crate) env: Arc<dyn Env>,
    pub(crate) locks: KeyedLocks,
    pub(crate) max_claims: usize,
}

impl Coord {
    pub fn new(repo: impl Into<String>, meta: Arc<dyn MetaStore>, env: Arc<dyn Env>) -> Self {
        Self {
            repo: repo.into(),
            meta,
            env,
            locks: KeyedLocks::default(),
            max_claims: 1,
        }
    }

    /// How many unfinished claims one principal may hold (DeLM uses 1, the default).
    pub fn with_max_claims(mut self, max_claims: usize) -> Self {
        self.max_claims = max_claims;
        self
    }

    pub fn env(&self) -> &dyn Env {
        self.env.as_ref()
    }

    pub(crate) fn save<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> Result<()> {
        Ok(self
            .meta
            .put_record(&self.repo, kind, id, &serde_json::to_vec(value)?)?)
    }

    pub(crate) fn load<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<Option<T>> {
        match self.meta.get_record(&self.repo, kind, id)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Every record of `kind`, in id order.
    pub(crate) fn load_all<T: DeserializeOwned>(&self, kind: &str) -> Result<Vec<T>> {
        self.meta
            .list_records(&self.repo, kind)?
            .into_iter()
            .map(|(_, bytes)| Ok(serde_json::from_slice(&bytes)?))
            .collect()
    }

    pub(crate) fn delete(&self, kind: &str, id: &str) -> Result<()> {
        Ok(self.meta.delete_record(&self.repo, kind, id)?)
    }
}

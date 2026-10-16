use std::sync::Mutex;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use trellis_core::{Changeset, Hash};
use trellis_store::{MetaStore, RefState, Result, StoreError};

/// A [`MetaStore`] that injects faults: a ref CAS that spuriously reports failure (as if
/// another writer won), and receipt writes that fail (as if the process crashed right
/// after landing).
pub struct FaultyMeta<M> {
    inner: M,
    rng: Mutex<ChaCha8Rng>,
    cas_fault: f64,
    receipt_fault: f64,
}

impl<M: MetaStore> FaultyMeta<M> {
    pub fn new(inner: M, seed: u64, cas_fault: f64, receipt_fault: f64) -> Self {
        Self {
            inner,
            rng: Mutex::new(ChaCha8Rng::seed_from_u64(seed)),
            cas_fault,
            receipt_fault,
        }
    }

    fn roll(&self, p: f64) -> bool {
        p > 0.0
            && self
                .rng
                .lock()
                .map(|mut r| r.random_bool(p))
                .unwrap_or(false)
    }
}

fn injected() -> StoreError {
    StoreError::Io(std::io::Error::other("injected fault"))
}

impl<M: MetaStore> MetaStore for FaultyMeta<M> {
    fn put_changeset(&self, repo: &str, cs: &Changeset) -> Result<Hash> {
        self.inner.put_changeset(repo, cs)
    }
    fn get_changeset(&self, repo: &str, id: &Hash) -> Result<Option<Changeset>> {
        self.inner.get_changeset(repo, id)
    }
    fn get_ref(&self, repo: &str, name: &str) -> Result<Option<RefState>> {
        self.inner.get_ref(repo, name)
    }
    fn cas_ref(&self, repo: &str, name: &str, expected: Option<Hash>, new: Hash) -> Result<bool> {
        if expected.is_some() && self.roll(self.cas_fault) {
            return Ok(false);
        }
        self.inner.cas_ref(repo, name, expected, new)
    }
    fn put_derived(
        &self,
        repo: &str,
        cs: &Hash,
        name: &str,
        version: u32,
        value: &[u8],
    ) -> Result<()> {
        self.inner.put_derived(repo, cs, name, version, value)
    }
    fn get_derived(
        &self,
        repo: &str,
        cs: &Hash,
        name: &str,
        version: u32,
    ) -> Result<Option<Vec<u8>>> {
        self.inner.get_derived(repo, cs, name, version)
    }
    fn put_record(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<()> {
        if kind == "receipt" && self.roll(self.receipt_fault) {
            return Err(injected());
        }
        self.inner.put_record(repo, kind, id, body)
    }
    fn put_record_if_absent(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<bool> {
        self.inner.put_record_if_absent(repo, kind, id, body)
    }
    fn get_record(&self, repo: &str, kind: &str, id: &str) -> Result<Option<Vec<u8>>> {
        self.inner.get_record(repo, kind, id)
    }
    fn list_records(&self, repo: &str, kind: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.inner.list_records(repo, kind)
    }
    fn delete_record(&self, repo: &str, kind: &str, id: &str) -> Result<()> {
        self.inner.delete_record(repo, kind, id)
    }
}

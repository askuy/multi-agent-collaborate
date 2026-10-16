# M1: Sessions, Landing, Conflicts, Simulation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let many agents work in private sessions and land their changes onto `main` without locks, with three-way merges on overlap, first-class conflicts, idempotent receipts, and a deterministic simulation that checks the spec's invariants.

**Architecture:** A new `trellis-land` crate holds an `Engine` per repository. A session is a base changeset plus an overlay of unlanded writes (spec §2, the ScorpioFS upper layer). `Engine::land` is the steward (spec §4.4, ATM): under the session's lock it reads `main`, scans what changed since the session's base, routes the overlay (parallel, compose by `merge3`, serialize by policy lock, or reject on a strict path, spec §4.2), and then moves `main` with one compare-and-swap (CAS), retrying when `main` moved. Unmergeable paths become a stored `Conflict` instead of a failure (spec §4.6, Jujutsu). Every land call is keyed by an idempotency key and returns a stored `Receipt` (spec §4.4, Oak). A landed changeset carries the session id in its provenance, so a retry after a crash between the CAS and the receipt write recovers the original result instead of landing twice. Sessions, receipts, and conflicts are serde-JSON records in a generic `records` table added to the metadata store. A new `trellis-sim` crate drives many virtual agents with a seeded RNG and fault injection, then checks invariants 1, 3, 6, and 8 (spec §7).

**Tech Stack:** Rust 2024; existing crates plus `serde`/`serde_json`, `globset` (policy globs), `parking_lot` (keyed locks), `rand` + `rand_chacha` (deterministic simulation).

**Spec:** `docs/superpowers/specs/2026-10-15-trellis-design.md` (§2, §4.2, §4.4, §4.6, §4.11 policy subset, §7 invariants 1, 3, 6, 8).

**Deliberately deferred to M2:** leases (the other fail-closed trigger), intents, read-set tracking and notifications, the context log, tasks. M1's `Strict` mode is a write policy: a strict path is never auto-merged.

---

## File Structure

```text
Cargo.toml                                   + serde, globset, parking_lot, rand, rand_chacha
crates/trellis-core/
  Cargo.toml                                 + serde dependency, serde_json dev-dependency
  src/hash.rs, src/path.rs                   + Serialize/Deserialize (hex string / validated string)
  src/kind.rs, src/changeset.rs              + serde derives
crates/trellis-store/
  src/meta.rs                                + record methods on MetaStore
  src/sqlite.rs                              + records table and implementation
crates/trellis-land/
  Cargo.toml
  src/lib.rs                                 re-exports
  src/error.rs                               LandError, Result
  src/env.rs                                 Env trait (time, ids), SystemEnv, TestEnv
  src/policy.rs                              WriteMode, Policy (glob → mode)
  src/records.rs                             SessionRecord, Receipt, Outcome, Route, ConflictRecord, encode/decode
  src/locks.rs                               KeyedLocks (sorted multi-key locking)
  src/engine/mod.rs                          Engine: init, sessions, write/read/close, blobs
  src/engine/merge.rs                        scan_since, merge_overlay, collision check
  src/engine/land.rs                         land (steward loop, receipts, recovery)
  src/engine/conflict.rs                     conflicts, rebase, resolve_conflict
  tests/concurrency.rs                       real-thread stress test
crates/trellis-sim/
  Cargo.toml
  src/lib.rs                                 SimConfig, SimReport, run
  src/faulty.rs                              FaultyMeta: injected CAS and receipt-write failures
  src/agent.rs                               virtual agent behavior
  src/check.rs                               invariant checks
  src/main.rs                                CLI: run many seeds
```

---

### Task 1: Serde for core types

**Files:**
- Modify: `Cargo.toml`, `crates/trellis-core/Cargo.toml`, `crates/trellis-core/src/hash.rs`, `crates/trellis-core/src/path.rs`, `crates/trellis-core/src/kind.rs`, `crates/trellis-core/src/changeset.rs`

- [ ] **Step 1: Add workspace dependencies**

In `Cargo.toml` under `[workspace.dependencies]`, add after `proptest = "1"`:

```toml
trellis-land = { path = "crates/trellis-land" }
serde = { version = "1", features = ["derive"] }
globset = "0.4"
parking_lot = { version = "0.12", features = ["arc_lock"] }
rand = "0.9"
rand_chacha = "0.9"
```

In `crates/trellis-core/Cargo.toml`, add `serde = { workspace = true }` under `[dependencies]` and `serde_json = { workspace = true }` under `[dev-dependencies]`.

- [ ] **Step 2: Write the failing test**

Append to the `tests` module in `crates/trellis-core/src/changeset.rs`:

```rust
    #[test]
    fn serializes_to_readable_json_and_back() {
        let cs = sample();
        let json = serde_json::to_value(&cs).unwrap();
        assert_eq!(json["parents"][0], serde_json::json!(Hash::of(b"parent").to_hex()));
        assert_eq!(json["changes"]["docs/a.md"]["op"], "upsert");
        assert_eq!(json["changes"]["docs/a.md"]["kind"], "markdown");
        assert_eq!(json["changes"]["old.txt"]["op"], "delete");
        let back: Changeset = serde_json::from_value(json).unwrap();
        assert_eq!(back, cs);

        let bad = serde_json::json!({"op": "delete"});
        assert!(serde_json::from_value::<FileChange>(bad).is_ok());
        assert!(serde_json::from_str::<RepoPath>("\"../x\"").is_err());
        assert!(serde_json::from_str::<Hash>("\"abc\"").is_err());
    }
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -p trellis-core serializes`
Expected: compile error, `the trait bound 'Changeset: Serialize' is not satisfied`.

- [ ] **Step 4: Implement serde support**

Append to `crates/trellis-core/src/hash.rs` (before `#[cfg(test)]`):

```rust
impl serde::Serialize for Hash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for Hash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
```

Append to `crates/trellis-core/src/path.rs` (before `#[cfg(test)]`):

```rust
impl serde::Serialize for RepoPath {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for RepoPath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
```

In `crates/trellis-core/src/kind.rs`, change the derive on `FileKind` to:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
```

In `crates/trellis-core/src/changeset.rs`, change the derives:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlobRef {
```

```rust
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum FileChange {
```

```rust
#[derive(Clone, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Provenance {
```

```rust
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Changeset {
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p trellis-core`
Expected: `test result: ok. 15 passed`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/trellis-core
git commit -m "feat(core): add serde support for core types"
```

---

### Task 2: Generic records in the metadata store

**Files:**
- Modify: `crates/trellis-store/src/meta.rs`, `crates/trellis-store/src/sqlite.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `crates/trellis-store/src/sqlite.rs`:

```rust
    #[test]
    fn records_upsert_insert_once_and_list_in_id_order() {
        let s = SqliteMetaStore::open_in_memory().unwrap();
        assert_eq!(s.get_record("r", "session", "b").unwrap(), None);
        s.put_record("r", "session", "b", b"1").unwrap();
        s.put_record("r", "session", "b", b"2").unwrap();
        s.put_record("r", "session", "a", b"x").unwrap();
        s.put_record("r", "receipt", "a", b"y").unwrap();
        s.put_record("other", "session", "c", b"z").unwrap();
        assert_eq!(s.get_record("r", "session", "b").unwrap().as_deref(), Some(&b"2"[..]));
        assert_eq!(
            s.list_records("r", "session").unwrap(),
            vec![("a".to_owned(), b"x".to_vec()), ("b".to_owned(), b"2".to_vec())]
        );
        assert!(s.put_record_if_absent("r", "receipt", "new", b"1").unwrap());
        assert!(!s.put_record_if_absent("r", "receipt", "new", b"2").unwrap());
        assert_eq!(s.get_record("r", "receipt", "new").unwrap().as_deref(), Some(&b"1"[..]));
        s.delete_record("r", "receipt", "new").unwrap();
        assert_eq!(s.get_record("r", "receipt", "new").unwrap(), None);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p trellis-store records`
Expected: compile error, `no method named 'get_record' found`.

- [ ] **Step 3: Add the trait methods and the SQLite implementation**

Append inside `pub trait MetaStore` in `crates/trellis-store/src/meta.rs`:

```rust
    /// Upserts an opaque record (sessions, receipts, conflicts, ...) keyed by `(kind, id)`.
    fn put_record(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<()>;
    /// Inserts a record only if absent; returns whether it was inserted.
    fn put_record_if_absent(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<bool>;
    fn get_record(&self, repo: &str, kind: &str, id: &str) -> Result<Option<Vec<u8>>>;
    /// Every record of `kind`, ordered by id.
    fn list_records(&self, repo: &str, kind: &str) -> Result<Vec<(String, Vec<u8>)>>;
    fn delete_record(&self, repo: &str, kind: &str, id: &str) -> Result<()>;
```

In `crates/trellis-store/src/sqlite.rs`, append to `SCHEMA` (before the closing `";`):

```sql
CREATE TABLE IF NOT EXISTS records (
    repo TEXT NOT NULL,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    body BLOB NOT NULL,
    PRIMARY KEY (repo, kind, id)
) WITHOUT ROWID;
```

Append inside `impl MetaStore for SqliteMetaStore`:

```rust
    fn put_record(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO records (repo, kind, id, body) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (repo, kind, id) DO UPDATE SET body = excluded.body",
            params![repo, kind, id, body],
        )?;
        Ok(())
    }

    fn put_record_if_absent(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<bool> {
        let inserted = self.conn()?.execute(
            "INSERT OR IGNORE INTO records (repo, kind, id, body) VALUES (?1, ?2, ?3, ?4)",
            params![repo, kind, id, body],
        )?;
        Ok(inserted == 1)
    }

    fn get_record(&self, repo: &str, kind: &str, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn()?
            .query_row(
                "SELECT body FROM records WHERE repo = ?1 AND kind = ?2 AND id = ?3",
                params![repo, kind, id],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn list_records(&self, repo: &str, kind: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT id, body FROM records WHERE repo = ?1 AND kind = ?2 ORDER BY id")?;
        let rows = stmt.query_map(params![repo, kind], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn delete_record(&self, repo: &str, kind: &str, id: &str) -> Result<()> {
        self.conn()?.execute(
            "DELETE FROM records WHERE repo = ?1 AND kind = ?2 AND id = ?3",
            params![repo, kind, id],
        )?;
        Ok(())
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-store`
Expected: `test result: ok. 10 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-store
git commit -m "feat(store): add generic records to the metadata store"
```

---

### Task 3: `trellis-land` skeleton: errors, environment, policy, locks

**Files:**
- Create: `crates/trellis-land/Cargo.toml`, `crates/trellis-land/src/lib.rs`, `crates/trellis-land/src/error.rs`, `crates/trellis-land/src/env.rs`, `crates/trellis-land/src/policy.rs`, `crates/trellis-land/src/locks.rs`

- [ ] **Step 1: Create the crate with failing tests**

`crates/trellis-land/Cargo.toml`:

```toml
[package]
name = "trellis-land"
description = "Trellis sessions and landing: admission routing, three-way merges, CAS on main, receipts, and conflicts."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-core = { workspace = true }
trellis-store = { workspace = true }
trellis-merge = { workspace = true }
trellis-derive = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
globset = { workspace = true }
parking_lot = { workspace = true }
```

`crates/trellis-land/src/lib.rs`:

```rust
//! Sessions and landing (spec §4.2, §4.4, §4.6): agents write into private sessions and the
//! engine lands them onto `main` with merges, a single CAS, receipts, and stored conflicts.

mod env;
mod error;
mod locks;
mod policy;

pub use env::{Env, SystemEnv, TestEnv};
pub use error::{LandError, Result};
pub use policy::{Policy, WriteMode};
```

`crates/trellis-land/src/error.rs`:

```rust
use trellis_core::CoreError;
use trellis_derive::DeriveError;
use trellis_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum LandError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Derive(#[from] DeriveError),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("record encoding: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("invalid policy pattern {pattern:?}: {message}")]
    Policy { pattern: String, message: String },
    #[error("repository is not initialized")]
    NotInitialized,
    #[error("session {0} not found")]
    SessionNotFound(String),
    #[error("session {0} is not open")]
    SessionNotOpen(String),
    #[error("conflict {0} not found")]
    ConflictNotFound(String),
    #[error("conflict {0} is no longer current")]
    ConflictNotCurrent(String),
    #[error("conflict {id} has unresolved paths: {paths:?}")]
    Unresolved { id: String, paths: Vec<String> },
    #[error("session base is not on main's history")]
    BaseNotOnMain,
    #[error("main kept moving; gave up after {0} attempts")]
    Contention(usize),
}

pub type Result<T> = std::result::Result<T, LandError>;
```

`crates/trellis-land/src/env.rs`:

```rust
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Time and identifiers, injectable so simulations are deterministic.
pub trait Env: Send + Sync {
    fn now_ms(&self) -> i64;
    /// A new identifier, unique within this repository.
    fn new_id(&self, prefix: &str) -> String;
}

/// Wall-clock time; ids are `<prefix>-<process start ms, hex>-<counter, hex>`.
pub struct SystemEnv {
    started_ms: i64,
    counter: AtomicU64,
}

/// Deterministic time (one millisecond per call) and sequential ids.
#[derive(Default)]
pub struct TestEnv {
    clock: AtomicI64,
    counter: AtomicU64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_is_deterministic() {
        let env = TestEnv::new();
        assert_eq!(env.new_id("s"), "s-0");
        assert_eq!(env.new_id("c"), "c-1");
        assert_eq!(env.now_ms(), 1);
        assert_eq!(env.now_ms(), 2);
    }

    #[test]
    fn system_env_ids_are_unique() {
        let env = SystemEnv::new();
        let a = env.new_id("s");
        let b = env.new_id("s");
        assert_ne!(a, b);
        assert!(a.starts_with("s-"));
        assert!(env.now_ms() > 1_600_000_000_000);
    }
}
```

`crates/trellis-land/src/policy.rs`:

```rust
use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};
use trellis_core::RepoPath;

use crate::error::{LandError, Result};

/// How landing treats a path that also changed on `main` since the session's base
/// (spec §4.2 admission routes).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WriteMode {
    /// Merge automatically by file kind (the compose route). The default.
    Optimistic,
    /// Merge automatically, but land one session at a time per matching pattern
    /// (lockfiles, migrations).
    Serialize,
    /// Never merge automatically: landing is rejected (fail-closed) if the path changed on
    /// `main` since the session's base. The agent must rebase and re-check its work.
    Strict,
}

/// Ordered glob rules; the first matching rule decides a path's mode.
#[derive(Clone, Debug, Default)]
pub struct Policy {
    rules: Vec<(String, GlobMatcher, WriteMode)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(p: &str) -> RepoPath {
        RepoPath::parse(p).unwrap()
    }

    #[test]
    fn first_matching_rule_wins_and_default_is_optimistic() {
        let policy = Policy::new(&[
            ("canon/secret.json", WriteMode::Strict),
            ("canon/**", WriteMode::Serialize),
            ("**/*.lock", WriteMode::Serialize),
        ])
        .unwrap();
        assert_eq!(policy.mode(&path("canon/secret.json")), WriteMode::Strict);
        assert_eq!(policy.mode(&path("canon/world.json")), WriteMode::Serialize);
        assert_eq!(policy.mode(&path("a/Cargo.lock")), WriteMode::Serialize);
        assert_eq!(policy.mode(&path("src/lib.rs")), WriteMode::Optimistic);
        assert_eq!(Policy::default().mode(&path("x")), WriteMode::Optimistic);
    }

    #[test]
    fn serialize_keys_are_sorted_and_unique() {
        let policy =
            Policy::new(&[("canon/**", WriteMode::Serialize), ("*.lock", WriteMode::Serialize)]).unwrap();
        let paths = [path("canon/a.json"), path("x.lock"), path("canon/b.json"), path("src/a.rs")];
        assert_eq!(policy.serialize_keys(paths.iter()), ["*.lock", "canon/**"]);
    }

    #[test]
    fn invalid_patterns_are_reported() {
        let err = Policy::new(&[("a[", WriteMode::Strict)]).unwrap_err();
        assert!(matches!(err, LandError::Policy { pattern, .. } if pattern == "a["));
    }
}
```

`crates/trellis-land/src/locks.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{ArcMutexGuard, Mutex, RawMutex};

pub(crate) type Guard = ArcMutexGuard<RawMutex, ()>;

/// Named in-process locks. Several keys are always taken in sorted order, so two callers
/// can never deadlock on each other.
#[derive(Default)]
pub(crate) struct KeyedLocks {
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn keys_exclude_each_other_and_overlapping_sets_do_not_deadlock() {
        let locks = Arc::new(KeyedLocks::default());
        let inside = Arc::new(AtomicUsize::new(0));
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let (locks, inside) = (Arc::clone(&locks), Arc::clone(&inside));
                std::thread::spawn(move || {
                    let keys = if i % 2 == 0 { vec!["b".to_owned(), "a".to_owned()] } else { vec!["a".to_owned(), "b".to_owned()] };
                    for _ in 0..200 {
                        let _guards = locks.lock(&keys);
                        assert_eq!(inside.fetch_add(1, Ordering::SeqCst), 0);
                        inside.fetch_sub(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-land`
Expected: compile errors, including `no function or associated item named 'new' found for struct 'TestEnv'`.

- [ ] **Step 3: Implement the environment, policy, and locks**

Insert into `crates/trellis-land/src/env.rs` before `#[cfg(test)]`:

```rust
impl SystemEnv {
    pub fn new() -> Self {
        let started_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self { started_ms, counter: AtomicU64::new(0) }
    }
}

impl Default for SystemEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl Env for SystemEnv {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn new_id(&self, prefix: &str) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{:x}-{n:x}", self.started_ms)
    }
}

impl TestEnv {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Env for TestEnv {
    fn now_ms(&self) -> i64 {
        self.clock.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn new_id(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.counter.fetch_add(1, Ordering::Relaxed))
    }
}
```

Insert into `crates/trellis-land/src/policy.rs` before `#[cfg(test)]`:

```rust
impl Policy {
    pub fn new(rules: &[(&str, WriteMode)]) -> Result<Self> {
        let rules = rules
            .iter()
            .map(|(pattern, mode)| {
                let matcher = Glob::new(pattern)
                    .map_err(|e| LandError::Policy {
                        pattern: (*pattern).to_owned(),
                        message: e.to_string(),
                    })?
                    .compile_matcher();
                Ok(((*pattern).to_owned(), matcher, *mode))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { rules })
    }

    fn rule(&self, path: &RepoPath) -> Option<&(String, GlobMatcher, WriteMode)> {
        self.rules.iter().find(|(_, matcher, _)| matcher.is_match(path.as_str()))
    }

    pub fn mode(&self, path: &RepoPath) -> WriteMode {
        self.rule(path).map_or(WriteMode::Optimistic, |(_, _, mode)| *mode)
    }

    /// The patterns of `Serialize` rules that decide any of `paths`, sorted and unique: the
    /// lock keys a landing must hold.
    pub fn serialize_keys<'a>(&self, paths: impl IntoIterator<Item = &'a RepoPath>) -> Vec<String> {
        let mut keys: Vec<String> = paths
            .into_iter()
            .filter_map(|path| match self.rule(path) {
                Some((pattern, _, WriteMode::Serialize)) => Some(pattern.clone()),
                _ => None,
            })
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }
}
```

Insert into `crates/trellis-land/src/locks.rs` before `#[cfg(test)]`:

```rust
impl KeyedLocks {
    /// Locks every key (sorted, deduplicated) and returns the guards; dropping them unlocks.
    pub(crate) fn lock(&self, keys: &[String]) -> Vec<Guard> {
        let mut keys = keys.to_vec();
        keys.sort();
        keys.dedup();
        keys.iter()
            .map(|key| {
                let mutex = Arc::clone(self.locks.lock().entry(key.clone()).or_default());
                mutex.lock_arc()
            })
            .collect()
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-land`
Expected: `test result: ok. 6 passed`.

- [ ] **Step 5: Commit**

```bash
git add Cargo.lock crates/trellis-land
git commit -m "feat(land): add errors, deterministic env, write policy, and keyed locks"
```

---

### Task 4: Records

**Files:**
- Create: `crates/trellis-land/src/records.rs`
- Modify: `crates/trellis-land/src/lib.rs`

- [ ] **Step 1: Write the records with a failing round-trip test**

`crates/trellis-land/src/records.rs`:

```rust
use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use trellis_core::{BlobRef, FileChange, FileKind, Hash, RepoPath};

use crate::error::Result;

pub(crate) const SESSION: &str = "session";
pub(crate) const RECEIPT: &str = "receipt";
pub(crate) const CONFLICT: &str = "conflict";

/// A session's unlanded writes, relative to its base.
pub type Overlay = BTreeMap<RepoPath, FileChange>;

/// Final contents for conflicting paths: `Some(bytes)` to write, `None` to delete.
pub type Resolutions = BTreeMap<RepoPath, Option<Vec<u8>>>;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum SessionStatus {
    Open,
    Landed { changeset: Hash },
    Closed,
}

/// One agent's workspace for one task: a base on `main` plus unlanded writes.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub principal: String,
    pub task: Option<String>,
    pub base: Hash,
    pub status: SessionStatus,
    pub overlay: Overlay,
    pub created_at_ms: i64,
}

/// How a landing reached `main`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Route {
    /// No path the session wrote changed on `main` since its base.
    Parallel,
    /// At least one path was merged with changes on `main`.
    Composed,
    /// Landed while holding a serialize lock from the policy.
    Serialized,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum Outcome {
    Landed { changeset: Hash, seq: u64, route: Route },
    /// Everything the session wrote was already on `main`; nothing new landed.
    Unchanged { main: Hash },
    /// Some paths could not be merged; see the conflict record. The session stays open.
    Conflicted { conflict: String },
    /// Fail-closed: the policy or the tree shape forbids this landing. The session stays open.
    Rejected { reason: String },
}

/// The stored result of one `land` call, returned again for a retry with the same key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub session: String,
    pub idempotency_key: String,
    pub outcome: Outcome,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConflictPath {
    pub path: RepoPath,
    pub kind: FileKind,
    pub base: Option<BlobRef>,
    pub ours: Option<BlobRef>,
    pub theirs: Option<BlobRef>,
    pub reason: String,
    pub detail: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictStatus {
    Open,
    Resolved,
}

/// A first-class conflict (spec §4.6): the session's base, the `main` it failed to merge
/// with, and the three versions of every unmergeable path.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub id: String,
    pub session: String,
    pub base: Hash,
    pub theirs: Hash,
    pub paths: Vec<ConflictPath>,
    pub status: ConflictStatus,
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    Ok(serde_json::from_slice(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip_through_json() {
        let receipt = Receipt {
            session: "s-1".into(),
            idempotency_key: "k".into(),
            outcome: Outcome::Landed { changeset: Hash::of(b"c"), seq: 3, route: Route::Composed },
        };
        let back: Receipt = decode(&encode(&receipt).unwrap()).unwrap();
        assert_eq!(back, receipt);
        let json: serde_json::Value = serde_json::from_slice(&encode(&receipt).unwrap()).unwrap();
        assert_eq!(json["outcome"]["outcome"], "landed");
        assert_eq!(json["outcome"]["route"], "composed");

        let session = SessionRecord {
            id: "s-1".into(),
            principal: "agent".into(),
            task: None,
            base: Hash::of(b"b"),
            status: SessionStatus::Landed { changeset: Hash::of(b"c") },
            overlay: Overlay::new(),
            created_at_ms: 5,
        };
        let back: SessionRecord = decode(&encode(&session).unwrap()).unwrap();
        assert_eq!(back, session);
    }
}
```

Modify `crates/trellis-land/src/lib.rs`:

```rust
//! Sessions and landing (spec §4.2, §4.4, §4.6): agents write into private sessions and the
//! engine lands them onto `main` with merges, a single CAS, receipts, and stored conflicts.

mod env;
mod error;
mod locks;
mod policy;
mod records;

pub use env::{Env, SystemEnv, TestEnv};
pub use error::{LandError, Result};
pub use policy::{Policy, WriteMode};
pub use records::{
    ConflictPath, ConflictRecord, ConflictStatus, Outcome, Overlay, Receipt, Resolutions, Route,
    SessionRecord, SessionStatus,
};
```

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cargo test -p trellis-land records`
Expected: `test result: ok. 1 passed` (the types and helpers are written together with the test; the test fails only if serde attributes are wrong).

- [ ] **Step 3: Commit**

```bash
git add crates/trellis-land
git commit -m "feat(land): add session, receipt, and conflict records"
```

---

### Task 5: `Engine` basics: init, sessions, write, read, close

**Files:**
- Create: `crates/trellis-land/src/engine/mod.rs`
- Modify: `crates/trellis-land/src/lib.rs`

- [ ] **Step 1: Write the engine skeleton and failing tests**

`crates/trellis-land/src/engine/mod.rs`:

```rust
use std::collections::BTreeMap;
use std::sync::Arc;

use trellis_core::{Changeset, FileChange, FileKind, Hash, Provenance, RepoPath};
use trellis_derive::{DeriveCtx, DeriveError, read_file};
use trellis_store::{BlobStore, MetaStore};

use crate::env::Env;
use crate::error::{LandError, Result};
use crate::locks::{Guard, KeyedLocks};
use crate::policy::Policy;
use crate::records::{SESSION, SessionRecord, SessionStatus, decode, encode};

/// The ref every session lands on.
pub const MAIN: &str = "main";
/// How many times a landing re-reads `main` after losing the CAS before giving up.
pub const MAX_LAND_ATTEMPTS: usize = 32;

/// Sessions and landing for one repository.
pub struct Engine {
    repo: String,
    meta: Arc<dyn MetaStore>,
    blobs: Arc<dyn BlobStore>,
    env: Arc<dyn Env>,
    policy: Policy,
    locks: KeyedLocks,
}

#[cfg(test)]
pub(crate) mod tests {
    use trellis_store::{MemBlobStore, SqliteMetaStore};

    use super::*;
    use crate::env::TestEnv;

    pub(crate) fn engine_with(policy: Policy) -> Engine {
        let engine = Engine::new(
            "repo",
            Arc::new(SqliteMetaStore::open_in_memory().unwrap()),
            Arc::new(MemBlobStore::new()),
            Arc::new(TestEnv::new()),
            policy,
        );
        engine.init().unwrap();
        engine
    }

    pub(crate) fn engine() -> Engine {
        engine_with(Policy::default())
    }

    pub(crate) fn p(path: &str) -> RepoPath {
        RepoPath::parse(path).unwrap()
    }

    pub(crate) fn text(bytes: Option<Vec<u8>>) -> Option<String> {
        bytes.map(|b| String::from_utf8(b).unwrap())
    }

    #[test]
    fn init_is_idempotent_and_main_starts_empty() {
        let e = engine();
        let root = e.main().unwrap();
        assert_eq!(e.init().unwrap(), root);
        assert_eq!(e.read_main(&p("a.txt")).unwrap(), None);
    }

    #[test]
    fn sessions_see_their_own_writes_over_their_base() {
        let e = engine();
        let s = e.open_session("alice", Some("T-1")).unwrap();
        assert_eq!(s.base, e.main().unwrap());
        assert_eq!(s.status, SessionStatus::Open);
        e.write(&s.id, &p("a.txt"), Some(b"hello")).unwrap();
        assert_eq!(text(e.read(&s.id, &p("a.txt")).unwrap()).as_deref(), Some("hello"));
        e.write(&s.id, &p("a.txt"), None).unwrap();
        assert_eq!(e.read(&s.id, &p("a.txt")).unwrap(), None);
        // Nothing reached main.
        assert_eq!(e.read_main(&p("a.txt")).unwrap(), None);
    }

    #[test]
    fn closed_and_unknown_sessions_refuse_writes() {
        let e = engine();
        let s = e.open_session("alice", None).unwrap();
        e.close_session(&s.id).unwrap();
        assert!(matches!(e.write(&s.id, &p("a"), Some(b"x")), Err(LandError::SessionNotOpen(_))));
        assert!(matches!(e.session("nope"), Err(LandError::SessionNotFound(_))));
    }
}
```

Modify `crates/trellis-land/src/lib.rs`: add `mod engine;` after `mod env;`, and `pub use engine::{Engine, MAIN, MAX_LAND_ATTEMPTS};` after the `env` re-export.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-land engine`
Expected: compile error, `no function or associated item named 'new' found for struct 'Engine'`.

- [ ] **Step 3: Implement the basics**

Insert into `crates/trellis-land/src/engine/mod.rs` before `#[cfg(test)]`:

```rust
pub(crate) fn session_key(id: &str) -> String {
    format!("session:{id}")
}

impl Engine {
    pub fn new(
        repo: impl Into<String>,
        meta: Arc<dyn MetaStore>,
        blobs: Arc<dyn BlobStore>,
        env: Arc<dyn Env>,
        policy: Policy,
    ) -> Self {
        Self { repo: repo.into(), meta, blobs, env, policy, locks: KeyedLocks::default() }
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }

    pub(crate) fn ctx(&self) -> DeriveCtx<'_> {
        DeriveCtx::new(&self.repo, self.meta.as_ref(), self.blobs.as_ref())
    }

    pub(crate) fn lock_session(&self, id: &str) -> Vec<Guard> {
        self.locks.lock(&[session_key(id)])
    }

    /// Creates the empty root changeset and `main` if the repository has none; returns `main`.
    pub fn init(&self) -> Result<Hash> {
        if let Some(state) = self.meta.get_ref(&self.repo, MAIN)? {
            return Ok(state.target);
        }
        let root = Changeset {
            parents: Vec::new(),
            changes: BTreeMap::new(),
            provenance: Provenance { principal: "trellis".into(), ..Default::default() },
            message: "root".into(),
            created_at_ms: 0,
        };
        let id = self.meta.put_changeset(&self.repo, &root)?;
        // Losing this CAS means another caller initialized the repository first.
        self.meta.cas_ref(&self.repo, MAIN, None, id)?;
        self.main()
    }

    pub fn main(&self) -> Result<Hash> {
        self.meta
            .get_ref(&self.repo, MAIN)?
            .map(|state| state.target)
            .ok_or(LandError::NotInitialized)
    }

    pub fn open_session(&self, principal: &str, task: Option<&str>) -> Result<SessionRecord> {
        let session = SessionRecord {
            id: self.env.new_id("s"),
            principal: principal.to_owned(),
            task: task.map(str::to_owned),
            base: self.main()?,
            status: SessionStatus::Open,
            overlay: BTreeMap::new(),
            created_at_ms: self.env.now_ms(),
        };
        self.save_session(&session)?;
        Ok(session)
    }

    pub fn session(&self, id: &str) -> Result<SessionRecord> {
        let bytes = self
            .meta
            .get_record(&self.repo, SESSION, id)?
            .ok_or_else(|| LandError::SessionNotFound(id.to_owned()))?;
        decode(&bytes)
    }

    pub(crate) fn save_session(&self, session: &SessionRecord) -> Result<()> {
        Ok(self.meta.put_record(&self.repo, SESSION, &session.id, &encode(session)?)?)
    }

    pub(crate) fn open_session_record(&self, id: &str) -> Result<SessionRecord> {
        let session = self.session(id)?;
        if session.status != SessionStatus::Open {
            return Err(LandError::SessionNotOpen(id.to_owned()));
        }
        Ok(session)
    }

    /// Writes `content` (or deletes, with `None`) at `path` in the session's overlay.
    pub fn write(&self, session: &str, path: &RepoPath, content: Option<&[u8]>) -> Result<()> {
        let _guard = self.lock_session(session);
        let mut record = self.open_session_record(session)?;
        let change = match content {
            Some(bytes) => FileChange::Upsert {
                blob: self.blobs.put(bytes)?,
                kind: FileKind::infer(path, bytes),
            },
            None => FileChange::Delete,
        };
        record.overlay.insert(path.clone(), change);
        self.save_session(&record)
    }

    /// Reads `path` as the session sees it: its own writes over its base.
    pub fn read(&self, session: &str, path: &RepoPath) -> Result<Option<Vec<u8>>> {
        let record = self.session(session)?;
        match record.overlay.get(path) {
            Some(FileChange::Upsert { blob, .. }) => Ok(Some(self.read_blob(&blob.hash)?)),
            Some(FileChange::Delete) => Ok(None),
            None => Ok(read_file(&self.ctx(), &record.base, path)?),
        }
    }

    pub fn read_main(&self, path: &RepoPath) -> Result<Option<Vec<u8>>> {
        Ok(read_file(&self.ctx(), &self.main()?, path)?)
    }

    pub fn read_blob(&self, hash: &Hash) -> Result<Vec<u8>> {
        self.blobs.get(hash)?.ok_or(LandError::Derive(DeriveError::MissingObject(*hash)))
    }

    /// Abandons an open session; its writes are discarded.
    pub fn close_session(&self, session: &str) -> Result<()> {
        let _guard = self.lock_session(session);
        let mut record = self.open_session_record(session)?;
        record.status = SessionStatus::Closed;
        record.overlay.clear();
        self.save_session(&record)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-land`
Expected: `test result: ok. 10 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-land
git commit -m "feat(land): add Engine with sessions, overlay writes, and reads"
```

---

### Task 6: Merging helpers: scan, overlay merge, collision check

**Files:**
- Create: `crates/trellis-land/src/engine/merge.rs`
- Modify: `crates/trellis-land/src/engine/mod.rs` (add `mod merge;`)

- [ ] **Step 1: Write the helpers with failing tests**

`crates/trellis-land/src/engine/merge.rs`:

```rust
use std::collections::BTreeSet;

use trellis_core::{BlobRef, FileChange, FileKind, Hash, RepoPath};
use trellis_derive::{DeriveCtx, TreeEntry, TreeIndex, is_first_parent_ancestor, lookup};
use trellis_merge::{MergeResult, merge3};

use super::Engine;
use crate::error::{LandError, Result};
use crate::policy::WriteMode;
use crate::records::{ConflictPath, Overlay, Resolutions};

/// What changed on `main` between a session's base and a target.
pub(crate) struct Scan {
    pub touched: BTreeSet<RepoPath>,
    /// A changeset in the range that this session already landed (a retry after a crash).
    pub already_landed: Option<Hash>,
}

pub(crate) enum MergeStep {
    Merged { overlay: Overlay, composed: bool },
    Conflicts(Vec<ConflictPath>),
    Strict(RepoPath),
}

fn file_at(ctx: &DeriveCtx<'_>, root: &Hash, path: &RepoPath) -> Result<Option<(BlobRef, FileKind)>> {
    Ok(match lookup(ctx, root, path)? {
        Some(TreeEntry::File { blob, kind }) => Some((blob, kind)),
        _ => None,
    })
}

/// Whether applying `change` leaves the path exactly as `current`.
fn is_noop(change: &FileChange, current: Option<(BlobRef, FileKind)>) -> bool {
    match (change, current) {
        (FileChange::Upsert { blob, .. }, Some((existing, _))) => blob.hash == existing.hash,
        (FileChange::Delete, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::{engine, p};

    #[test]
    fn collisions_between_files_and_directories_are_found() {
        let e = engine();
        // Put "dir/file.txt" and "top.txt" on main directly (landing arrives in Task 7).
        let blob = e.blobs.put(b"x").unwrap();
        let upsert = |path: &str| (p(path), FileChange::Upsert { blob, kind: FileKind::Text });
        let main = e.main().unwrap();
        let seed = trellis_core::Changeset {
            parents: vec![main],
            changes: [upsert("dir/file.txt"), upsert("top.txt")].into_iter().collect(),
            provenance: Default::default(),
            message: String::new(),
            created_at_ms: 0,
        };
        let id = e.meta.put_changeset(&e.repo, &seed).unwrap();
        assert!(e.meta.cas_ref(&e.repo, crate::engine::MAIN, Some(main), id).unwrap());

        let ctx = e.ctx();
        let root = ctx.get::<TreeIndex>(&e.main().unwrap()).unwrap().root;

        let inside_file: Overlay = [upsert("top.txt/child")].into_iter().collect();
        assert!(collision(&ctx, &root, &inside_file).unwrap().is_some());

        let over_dir: Overlay = [upsert("dir")].into_iter().collect();
        assert!(collision(&ctx, &root, &over_dir).unwrap().is_some());

        let file_then_child: Overlay = [upsert("new"), upsert("new/child")].into_iter().collect();
        assert!(collision(&ctx, &root, &file_then_child).unwrap().is_some());

        let replace_file_with_dir: Overlay =
            [(p("top.txt"), FileChange::Delete), upsert("top.txt/child")].into_iter().collect();
        assert_eq!(collision(&ctx, &root, &replace_file_with_dir).unwrap(), None);
    }
}
```

Add `mod merge;` near the top of `crates/trellis-land/src/engine/mod.rs` (after the `use` lines).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-land merge`
Expected: compile error, `cannot find function 'collision' in this scope`.

- [ ] **Step 3: Implement the helpers**

Insert into `crates/trellis-land/src/engine/merge.rs` before `#[cfg(test)]`:

```rust
/// Rejects a file and a directory sharing a path after `changes` are applied on `root`.
pub(crate) fn collision(ctx: &DeriveCtx<'_>, root: &Hash, changes: &Overlay) -> Result<Option<String>> {
    for (path, change) in changes {
        if !matches!(change, FileChange::Upsert { .. }) {
            continue;
        }
        let segments: Vec<&str> = path.segments().collect();
        for depth in 1..segments.len() {
            let prefix = RepoPath::parse(&segments[..depth].join("/"))?;
            let stays_a_file = match changes.get(&prefix) {
                Some(FileChange::Upsert { .. }) => true,
                Some(FileChange::Delete) => false,
                None => matches!(lookup(ctx, root, &prefix)?, Some(TreeEntry::File { .. })),
            };
            if stays_a_file {
                return Ok(Some(format!("{path} would be inside the file {prefix}")));
            }
        }
        if let Some(TreeEntry::Dir(_)) = lookup(ctx, root, path)? {
            return Ok(Some(format!("{path} is a directory on main")));
        }
    }
    Ok(None)
}

impl Engine {
    /// Walks `main`'s first-parent history from `target` back to `base`.
    pub(crate) fn scan_since(&self, ctx: &DeriveCtx<'_>, base: &Hash, target: &Hash, session: &str) -> Result<Scan> {
        if !is_first_parent_ancestor(ctx, base, target)? {
            return Err(LandError::BaseNotOnMain);
        }
        let mut touched = BTreeSet::new();
        let mut already_landed = None;
        let mut current = *target;
        while current != *base {
            let cs = ctx.changeset(&current)?;
            if already_landed.is_none() && cs.provenance.session.as_deref() == Some(session) {
                already_landed = Some(current);
            }
            touched.extend(cs.changes.keys().cloned());
            current = cs.first_parent().ok_or(LandError::BaseNotOnMain)?;
        }
        Ok(Scan { touched, already_landed })
    }

    /// Rebases `overlay` from `base` onto `target`: untouched paths carry over, touched paths
    /// are three-way merged by file kind (or taken from `resolutions`), and writes that would
    /// leave a path exactly as on `target` are dropped.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn merge_overlay(
        &self,
        ctx: &DeriveCtx<'_>,
        base: &Hash,
        target: &Hash,
        overlay: &Overlay,
        touched: &BTreeSet<RepoPath>,
        resolutions: &Resolutions,
        enforce_strict: bool,
    ) -> Result<MergeStep> {
        let base_root = ctx.get::<TreeIndex>(base)?.root;
        let target_root = ctx.get::<TreeIndex>(target)?.root;
        let mut merged = Overlay::new();
        let mut conflicts = Vec::new();
        let mut composed = false;
        for (path, change) in overlay {
            let theirs = file_at(ctx, &target_root, path)?;
            if !touched.contains(path) {
                if !is_noop(change, theirs) {
                    merged.insert(path.clone(), change.clone());
                }
                continue;
            }
            if enforce_strict && self.policy.mode(path) == WriteMode::Strict {
                return Ok(MergeStep::Strict(path.clone()));
            }
            composed = true;
            if let Some(resolution) = resolutions.get(path) {
                let resolved = match resolution {
                    Some(bytes) => FileChange::Upsert {
                        blob: self.blobs.put(bytes)?,
                        kind: FileKind::infer(path, bytes),
                    },
                    None => FileChange::Delete,
                };
                if !is_noop(&resolved, theirs) {
                    merged.insert(path.clone(), resolved);
                }
                continue;
            }
            let base_file = file_at(ctx, &base_root, path)?;
            let (ours_blob, kind) = match change {
                FileChange::Upsert { blob, kind } => (Some(*blob), *kind),
                FileChange::Delete => (None, theirs.or(base_file).map_or(FileKind::Text, |(_, k)| k)),
            };
            let load = |blob: Option<BlobRef>| blob.map(|b| self.read_blob(&b.hash)).transpose();
            let base_bytes = load(base_file.map(|(b, _)| b))?;
            let ours_bytes = load(ours_blob)?;
            let theirs_bytes = load(theirs.map(|(b, _)| b))?;
            match merge3(kind, base_bytes.as_deref(), ours_bytes.as_deref(), theirs_bytes.as_deref()) {
                MergeResult::Clean(result) => {
                    let change = match result {
                        Some(bytes) => FileChange::Upsert { blob: self.blobs.put(&bytes)?, kind },
                        None => FileChange::Delete,
                    };
                    if !is_noop(&change, theirs) {
                        merged.insert(path.clone(), change);
                    }
                }
                MergeResult::Conflict(conflict) => conflicts.push(ConflictPath {
                    path: path.clone(),
                    kind,
                    base: base_file.map(|(b, _)| b),
                    ours: ours_blob,
                    theirs: theirs.map(|(b, _)| b),
                    reason: format!("{:?}", conflict.reason),
                    detail: conflict.detail,
                }),
            }
        }
        if conflicts.is_empty() {
            Ok(MergeStep::Merged { overlay: merged, composed })
        } else {
            Ok(MergeStep::Conflicts(conflicts))
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-land`
Expected: `test result: ok. 11 passed`. Dead-code warnings for `Scan`, `MergeStep`, `scan_since`, and `merge_overlay` are expected until Task 7 uses them.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-land
git commit -m "feat(land): add history scan, overlay merge, and collision check"
```

---

### Task 7: Landing: the steward loop, receipts, recovery, conflicts

**Files:**
- Create: `crates/trellis-land/src/engine/land.rs`
- Modify: `crates/trellis-land/src/engine/mod.rs` (add `mod land;`)

- [ ] **Step 1: Write the failing tests**

`crates/trellis-land/src/engine/land.rs`:

```rust
use trellis_core::{Changeset, Hash, Provenance};
use trellis_derive::{SkipList, TreeIndex};

use super::merge::{MergeStep, collision};
use super::{Engine, MAIN, MAX_LAND_ATTEMPTS};
use crate::error::{LandError, Result};
use crate::records::{
    CONFLICT, ConflictPath, ConflictRecord, ConflictStatus, Outcome, RECEIPT, Receipt, Resolutions,
    Route, SessionRecord, SessionStatus, decode, encode,
};

#[cfg(test)]
mod tests {
    use trellis_derive::flatten;

    use super::*;
    use crate::engine::tests::{engine, engine_with, p, text};
    use crate::policy::{Policy, WriteMode};

    fn landed(outcome: &Outcome) -> (Hash, u64, Route) {
        match outcome {
            Outcome::Landed { changeset, seq, route } => (*changeset, *seq, *route),
            other => panic!("expected Landed, got {other:?}"),
        }
    }

    #[test]
    fn a_session_lands_in_parallel_and_main_advances() {
        let e = engine();
        let s = e.open_session("alice", Some("T-1")).unwrap();
        e.write(&s.id, &p("a.txt"), Some(b"A")).unwrap();
        let r = e.land(&s.id, "add a", "k1").unwrap();
        let (cs, seq, route) = landed(&r.outcome);
        assert_eq!((seq, route), (1, Route::Parallel));
        assert_eq!(e.main().unwrap(), cs);
        assert_eq!(text(e.read_main(&p("a.txt")).unwrap()).as_deref(), Some("A"));
        let session = e.session(&s.id).unwrap();
        assert_eq!(session.status, SessionStatus::Landed { changeset: cs });
        let changeset = e.ctx().changeset(&cs).unwrap();
        assert_eq!(changeset.provenance.session.as_deref(), Some(s.id.as_str()));
        assert_eq!(changeset.provenance.task.as_deref(), Some("T-1"));
    }

    #[test]
    fn retrying_with_the_same_key_returns_the_same_receipt() {
        let e = engine();
        let s = e.open_session("alice", None).unwrap();
        e.write(&s.id, &p("a.txt"), Some(b"A")).unwrap();
        let first = e.land(&s.id, "m", "k1").unwrap();
        let main = e.main().unwrap();
        let again = e.land(&s.id, "m", "k1").unwrap();
        assert_eq!(again, first);
        assert_eq!(e.main().unwrap(), main);
        // A new key on a landed session is refused.
        assert!(matches!(e.land(&s.id, "m", "k2"), Err(LandError::SessionNotOpen(_))));
    }

    #[test]
    fn disjoint_sessions_land_on_top_of_each_other() {
        let e = engine();
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("a.txt"), Some(b"A")).unwrap();
        e.write(&b.id, &p("b.txt"), Some(b"B")).unwrap();
        landed(&e.land(&a.id, "a", "k").unwrap().outcome);
        let (cs_b, seq_b, route_b) = landed(&e.land(&b.id, "b", "k").unwrap().outcome);
        assert_eq!((seq_b, route_b), (2, Route::Parallel));
        let ctx = e.ctx();
        let files = flatten(&ctx, &ctx.get::<TreeIndex>(&cs_b).unwrap().root).unwrap();
        assert_eq!(files.keys().map(|k| k.as_str()).collect::<Vec<_>>(), ["a.txt", "b.txt"]);
    }

    #[test]
    fn overlapping_text_edits_compose() {
        let e = engine();
        let seed = e.open_session("seed", None).unwrap();
        e.write(&seed.id, &p("doc.md"), Some(b"one\ntwo\nthree\nfour\nfive\n")).unwrap();
        landed(&e.land(&seed.id, "seed", "k").unwrap().outcome);

        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("doc.md"), Some(b"ONE\ntwo\nthree\nfour\nfive\n")).unwrap();
        e.write(&b.id, &p("doc.md"), Some(b"one\ntwo\nthree\nfour\nFIVE\n")).unwrap();
        landed(&e.land(&a.id, "a", "k").unwrap().outcome);
        let (_, _, route) = landed(&e.land(&b.id, "b", "k").unwrap().outcome);
        assert_eq!(route, Route::Composed);
        assert_eq!(
            text(e.read_main(&p("doc.md")).unwrap()).as_deref(),
            Some("ONE\ntwo\nthree\nfour\nFIVE\n")
        );
    }

    #[test]
    fn conflicting_edits_create_a_conflict_and_keep_the_session_open() {
        let e = engine();
        let seed = e.open_session("seed", None).unwrap();
        e.write(&seed.id, &p("doc.md"), Some(b"one\ntwo\nthree\n")).unwrap();
        landed(&e.land(&seed.id, "seed", "k").unwrap().outcome);

        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("doc.md"), Some(b"one\nTWO-A\nthree\n")).unwrap();
        e.write(&b.id, &p("doc.md"), Some(b"one\nTWO-B\nthree\n")).unwrap();
        let main_after_a = landed(&e.land(&a.id, "a", "k").unwrap().outcome).0;

        let r = e.land(&b.id, "b", "k").unwrap();
        let Outcome::Conflicted { conflict } = &r.outcome else { panic!("expected Conflicted, got {:?}", r.outcome) };
        let c = e.conflict(conflict).unwrap();
        assert_eq!(c.session, b.id);
        assert_eq!(c.theirs, main_after_a);
        assert_eq!(c.status, ConflictStatus::Open);
        assert_eq!(c.paths.len(), 1);
        assert_eq!(c.paths[0].path, p("doc.md"));
        assert!(c.paths[0].detail.contains("<<<<<<<"));
        assert_eq!(e.session(&b.id).unwrap().status, SessionStatus::Open);
        assert_eq!(e.main().unwrap(), main_after_a);
    }

    #[test]
    fn writes_already_on_main_land_as_unchanged() {
        let e = engine();
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("same.txt"), Some(b"S")).unwrap();
        e.write(&b.id, &p("same.txt"), Some(b"S")).unwrap();
        landed(&e.land(&a.id, "a", "k").unwrap().outcome);
        let main = e.main().unwrap();
        assert_eq!(e.land(&b.id, "b", "k").unwrap().outcome, Outcome::Unchanged { main });
        assert_eq!(e.session(&b.id).unwrap().status, SessionStatus::Closed);
    }

    #[test]
    fn strict_paths_are_never_merged() {
        let e = engine_with(Policy::new(&[("canon/**", WriteMode::Strict)]).unwrap());
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("canon/world.json"), Some(b"{\"a\":1}")).unwrap();
        e.write(&b.id, &p("canon/world.json"), Some(b"{\"b\":2}")).unwrap();
        landed(&e.land(&a.id, "a", "k").unwrap().outcome);
        let r = e.land(&b.id, "b", "k").unwrap();
        assert!(matches!(r.outcome, Outcome::Rejected { .. }), "{:?}", r.outcome);
        assert_eq!(e.session(&b.id).unwrap().status, SessionStatus::Open);
    }

    #[test]
    fn file_directory_collisions_are_rejected() {
        let e = engine();
        let a = e.open_session("a", None).unwrap();
        e.write(&a.id, &p("x"), Some(b"file")).unwrap();
        landed(&e.land(&a.id, "a", "k").unwrap().outcome);
        let b = e.open_session("b", None).unwrap();
        e.write(&b.id, &p("x/y"), Some(b"inside")).unwrap();
        assert!(matches!(e.land(&b.id, "b", "k").unwrap().outcome, Outcome::Rejected { .. }));
    }

    #[test]
    fn a_landing_whose_receipt_was_lost_is_recovered_not_repeated() {
        let e = engine();
        let s = e.open_session("a", None).unwrap();
        e.write(&s.id, &p("a.txt"), Some(b"A")).unwrap();
        let first = e.land(&s.id, "m", "k").unwrap();
        let (cs, seq, _) = landed(&first.outcome);

        // Simulate a crash right after the CAS: no receipt, session still open with its writes.
        e.forget_receipt(&s.id, "k").unwrap();
        let mut reopened = e.session(&s.id).unwrap();
        reopened.status = SessionStatus::Open;
        reopened.overlay = e.ctx().changeset(&cs).unwrap().changes;
        e.save_session(&reopened).unwrap();

        let retry = e.land(&s.id, "m", "k").unwrap();
        assert_eq!(landed(&retry.outcome), (cs, seq, Route::Parallel));
        assert_eq!(e.main().unwrap(), cs);
    }
}
```

`forget_receipt` is a `#[cfg(test)]` helper added in Step 3; it uses `delete_record` from Task 2.

Add `mod land;` next to `mod merge;` in `crates/trellis-land/src/engine/mod.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-land land`
Expected: compile error, `no method named 'land' found for struct 'Engine'`.

- [ ] **Step 3: Implement landing**

Insert into `crates/trellis-land/src/engine/land.rs` before `#[cfg(test)]`:

```rust
impl Engine {
    /// Lands the session onto `main` (spec §4.4). Retrying with the same `idempotency_key`
    /// returns the stored receipt and changes nothing.
    pub fn land(&self, session: &str, message: &str, idempotency_key: &str) -> Result<Receipt> {
        let _session_guard = self.lock_session(session);
        let receipt_id = format!("{session}/{idempotency_key}");
        if let Some(bytes) = self.meta.get_record(&self.repo, RECEIPT, &receipt_id)? {
            return decode(&bytes);
        }
        let record = self.open_session_record(session)?;
        let serialize_keys: Vec<String> = self
            .policy
            .serialize_keys(record.overlay.keys())
            .into_iter()
            .map(|pattern| format!("serialize:{pattern}"))
            .collect();
        let _serialize_guards = self.locks.lock(&serialize_keys);
        let outcome = self.land_locked(&record, message, !serialize_keys.is_empty())?;
        let receipt = Receipt {
            session: session.to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            outcome,
        };
        self.meta.put_record(&self.repo, RECEIPT, &receipt_id, &encode(&receipt)?)?;
        let status = match &receipt.outcome {
            Outcome::Landed { changeset, .. } => Some(SessionStatus::Landed { changeset: *changeset }),
            Outcome::Unchanged { .. } => Some(SessionStatus::Closed),
            Outcome::Conflicted { .. } | Outcome::Rejected { .. } => None,
        };
        if let Some(status) = status {
            let mut record = record;
            record.status = status;
            record.overlay.clear();
            self.save_session(&record)?;
        }
        Ok(receipt)
    }

    fn land_locked(&self, session: &SessionRecord, message: &str, serialized: bool) -> Result<Outcome> {
        let ctx = self.ctx();
        for _ in 0..MAX_LAND_ATTEMPTS {
            let main = self.main()?;
            let scan = self.scan_since(&ctx, &session.base, &main, &session.id)?;
            if let Some(changeset) = scan.already_landed {
                let seq = ctx.get::<SkipList>(&changeset)?.generation;
                return Ok(Outcome::Landed { changeset, seq, route: Route::Parallel });
            }
            let step = self.merge_overlay(
                &ctx,
                &session.base,
                &main,
                &session.overlay,
                &scan.touched,
                &Resolutions::new(),
                true,
            )?;
            let (changes, composed) = match step {
                MergeStep::Merged { overlay, composed } => (overlay, composed),
                MergeStep::Conflicts(paths) => {
                    let conflict = self.record_conflict(session, main, paths)?;
                    return Ok(Outcome::Conflicted { conflict });
                }
                MergeStep::Strict(path) => {
                    return Ok(Outcome::Rejected {
                        reason: format!(
                            "{path} changed on main since the session's base and its policy is strict; rebase and re-check"
                        ),
                    });
                }
            };
            if changes.is_empty() {
                return Ok(Outcome::Unchanged { main });
            }
            let root = ctx.get::<TreeIndex>(&main)?.root;
            if let Some(reason) = collision(&ctx, &root, &changes)? {
                return Ok(Outcome::Rejected { reason });
            }
            let changeset = Changeset {
                parents: vec![main],
                changes,
                provenance: Provenance {
                    principal: session.principal.clone(),
                    session: Some(session.id.clone()),
                    task: session.task.clone(),
                    model: None,
                    trace: None,
                },
                message: message.to_owned(),
                created_at_ms: self.env.now_ms(),
            };
            let id = self.meta.put_changeset(&self.repo, &changeset)?;
            if self.meta.cas_ref(&self.repo, MAIN, Some(main), id)? {
                let seq = ctx.get::<SkipList>(&id)?.generation;
                let route = if serialized {
                    Route::Serialized
                } else if composed {
                    Route::Composed
                } else {
                    Route::Parallel
                };
                return Ok(Outcome::Landed { changeset: id, seq, route });
            }
        }
        Err(LandError::Contention(MAX_LAND_ATTEMPTS))
    }

    pub(crate) fn record_conflict(
        &self,
        session: &SessionRecord,
        theirs: Hash,
        paths: Vec<ConflictPath>,
    ) -> Result<String> {
        let id = self.env.new_id("c");
        let record = ConflictRecord {
            id: id.clone(),
            session: session.id.clone(),
            base: session.base,
            theirs,
            paths,
            status: ConflictStatus::Open,
        };
        self.meta.put_record(&self.repo, CONFLICT, &id, &encode(&record)?)?;
        Ok(id)
    }

    pub fn conflict(&self, id: &str) -> Result<ConflictRecord> {
        let bytes = self
            .meta
            .get_record(&self.repo, CONFLICT, id)?
            .ok_or_else(|| LandError::ConflictNotFound(id.to_owned()))?;
        decode(&bytes)
    }

    #[cfg(test)]
    pub(crate) fn forget_receipt(&self, session: &str, key: &str) -> Result<()> {
        Ok(self.meta.delete_record(&self.repo, RECEIPT, &format!("{session}/{key}"))?)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-land`
Expected: `test result: ok. 20 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-land
git commit -m "feat(land): add the landing steward with receipts, recovery, and conflicts"
```

---

### Task 8: Rebase and conflict resolution

**Files:**
- Create: `crates/trellis-land/src/engine/conflict.rs`
- Modify: `crates/trellis-land/src/engine/mod.rs` (add `mod conflict;`), `crates/trellis-land/src/lib.rs` (export `RebaseOutcome`)

- [ ] **Step 1: Write the failing tests**

`crates/trellis-land/src/engine/conflict.rs`:

```rust
use serde::{Deserialize, Serialize};
use trellis_core::Hash;

use super::Engine;
use super::merge::MergeStep;
use crate::error::{LandError, Result};
use crate::records::{CONFLICT, ConflictRecord, ConflictStatus, Resolutions, SessionRecord, decode, encode};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum RebaseOutcome {
    /// The session now sits on `base` with its writes merged.
    Rebased { base: Hash },
    /// Some paths could not be merged; the session's base is unchanged.
    Conflicted { conflict: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::{engine, p, text};
    use crate::records::{Outcome, Route};

    fn seed(e: &Engine, path: &str, content: &[u8]) {
        let s = e.open_session("seed", None).unwrap();
        e.write(&s.id, &p(path), Some(content)).unwrap();
        assert!(matches!(e.land(&s.id, "seed", "k").unwrap().outcome, Outcome::Landed { .. }));
    }

    fn conflicted_pair(e: &Engine) -> (String, String) {
        seed(e, "doc.md", b"one\ntwo\nthree\n");
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("doc.md"), Some(b"one\nTWO-A\nthree\n")).unwrap();
        e.write(&b.id, &p("doc.md"), Some(b"one\nTWO-B\nthree\n")).unwrap();
        e.write(&b.id, &p("other.txt"), Some(b"untouched by a")).unwrap();
        assert!(matches!(e.land(&a.id, "a", "k").unwrap().outcome, Outcome::Landed { .. }));
        let Outcome::Conflicted { conflict } = e.land(&b.id, "b", "k").unwrap().outcome else {
            panic!("expected a conflict")
        };
        (b.id, conflict)
    }

    #[test]
    fn resolving_a_conflict_rebases_the_session_and_it_then_lands() {
        let e = engine();
        let (session, conflict) = conflicted_pair(&e);
        let mut resolutions = Resolutions::new();
        resolutions.insert(p("doc.md"), Some(b"one\nTWO-A+B\nthree\n".to_vec()));
        let out = e.resolve_conflict(&conflict, &resolutions).unwrap();
        assert_eq!(out, RebaseOutcome::Rebased { base: e.main().unwrap() });
        assert_eq!(e.conflict(&conflict).unwrap().status, ConflictStatus::Resolved);

        let r = e.land(&session, "b resolved", "k2").unwrap();
        let Outcome::Landed { route, .. } = r.outcome else { panic!("{:?}", r.outcome) };
        assert_eq!(route, Route::Parallel);
        assert_eq!(text(e.read_main(&p("doc.md")).unwrap()).as_deref(), Some("one\nTWO-A+B\nthree\n"));
        assert_eq!(text(e.read_main(&p("other.txt")).unwrap()).as_deref(), Some("untouched by a"));
    }

    #[test]
    fn resolutions_must_cover_every_conflicting_path_and_apply_once() {
        let e = engine();
        let (_, conflict) = conflicted_pair(&e);
        assert!(matches!(
            e.resolve_conflict(&conflict, &Resolutions::new()),
            Err(LandError::Unresolved { .. })
        ));
        let mut resolutions = Resolutions::new();
        resolutions.insert(p("doc.md"), None);
        e.resolve_conflict(&conflict, &resolutions).unwrap();
        assert!(matches!(
            e.resolve_conflict(&conflict, &resolutions),
            Err(LandError::ConflictNotCurrent(_))
        ));
        assert!(e.conflicts(true).unwrap().is_empty());
        assert_eq!(e.conflicts(false).unwrap().len(), 1);
    }

    #[test]
    fn rebase_moves_the_base_and_merges_clean_changes() {
        let e = engine();
        seed(&e, "doc.md", b"one\ntwo\nthree\nfour\nfive\n");
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("doc.md"), Some(b"ONE\ntwo\nthree\nfour\nfive\n")).unwrap();
        e.write(&b.id, &p("doc.md"), Some(b"one\ntwo\nthree\nfour\nFIVE\n")).unwrap();
        assert!(matches!(e.land(&a.id, "a", "k").unwrap().outcome, Outcome::Landed { .. }));
        let main = e.main().unwrap();
        assert_eq!(e.rebase(&b.id).unwrap(), RebaseOutcome::Rebased { base: main });
        assert_eq!(e.session(&b.id).unwrap().base, main);
        assert_eq!(
            text(e.read(&b.id, &p("doc.md")).unwrap()).as_deref(),
            Some("ONE\ntwo\nthree\nfour\nFIVE\n")
        );
    }
}
```

Add `mod conflict;` next to the other engine modules, and in `crates/trellis-land/src/lib.rs` change the engine re-export to `pub use engine::{Engine, MAIN, MAX_LAND_ATTEMPTS, RebaseOutcome};` and add `pub use conflict::RebaseOutcome;` inside `engine/mod.rs`:

```rust
mod conflict;
mod land;
mod merge;

pub use conflict::RebaseOutcome;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-land conflict`
Expected: compile error, `no method named 'resolve_conflict' found`.

- [ ] **Step 3: Implement rebase and resolution**

Insert into `crates/trellis-land/src/engine/conflict.rs` before `#[cfg(test)]`:

```rust
impl Engine {
    /// Every conflict, oldest id first; `open_only` filters out resolved ones.
    pub fn conflicts(&self, open_only: bool) -> Result<Vec<ConflictRecord>> {
        let mut out = Vec::new();
        for (_, bytes) in self.meta.list_records(&self.repo, CONFLICT)? {
            let record: ConflictRecord = decode(&bytes)?;
            if !open_only || record.status == ConflictStatus::Open {
                out.push(record);
            }
        }
        Ok(out)
    }

    /// Moves the session's base to the latest `main`, merging its writes. Unmergeable paths
    /// become a new conflict and leave the session where it was.
    pub fn rebase(&self, session: &str) -> Result<RebaseOutcome> {
        let _guard = self.lock_session(session);
        let record = self.open_session_record(session)?;
        let target = self.main()?;
        self.rebase_locked(record, target, &Resolutions::new())
    }

    /// Resolves a conflict: the session is rebased onto the `main` it conflicted with, using
    /// `resolutions` for the conflicting paths. Land it again afterwards (with a new key).
    pub fn resolve_conflict(&self, conflict: &str, resolutions: &Resolutions) -> Result<RebaseOutcome> {
        let mut record = self.conflict(conflict)?;
        let _guard = self.lock_session(&record.session);
        let session = self.open_session_record(&record.session)?;
        if record.status != ConflictStatus::Open || session.base != record.base {
            return Err(LandError::ConflictNotCurrent(conflict.to_owned()));
        }
        let missing: Vec<String> = record
            .paths
            .iter()
            .filter(|p| !resolutions.contains_key(&p.path))
            .map(|p| p.path.to_string())
            .collect();
        if !missing.is_empty() {
            return Err(LandError::Unresolved { id: conflict.to_owned(), paths: missing });
        }
        let outcome = self.rebase_locked(session, record.theirs, resolutions)?;
        if matches!(outcome, RebaseOutcome::Rebased { .. }) {
            record.status = ConflictStatus::Resolved;
            self.meta.put_record(self.repo(), CONFLICT, &record.id, &encode(&record)?)?;
        }
        Ok(outcome)
    }

    fn rebase_locked(&self, mut session: SessionRecord, target: Hash, resolutions: &Resolutions) -> Result<RebaseOutcome> {
        let ctx = self.ctx();
        let scan = self.scan_since(&ctx, &session.base, &target, &session.id)?;
        let step = self.merge_overlay(&ctx, &session.base, &target, &session.overlay, &scan.touched, resolutions, false)?;
        match step {
            MergeStep::Merged { overlay, .. } => {
                session.base = target;
                session.overlay = overlay;
                self.save_session(&session)?;
                Ok(RebaseOutcome::Rebased { base: target })
            }
            MergeStep::Conflicts(paths) => {
                let conflict = self.record_conflict(&session, target, paths)?;
                Ok(RebaseOutcome::Conflicted { conflict })
            }
            MergeStep::Strict(_) => unreachable!("merge_overlay only reports Strict when enforce_strict is set"),
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-land`
Expected: `test result: ok. 23 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-land
git commit -m "feat(land): add rebase and conflict resolution"
```

---

### Task 9: Real-thread concurrency test (serialize route, CAS contention)

**Files:**
- Create: `crates/trellis-land/tests/concurrency.rs`
- Modify: `crates/trellis-land/Cargo.toml` (dev-dependencies)

- [ ] **Step 1: Write the test**

Add to `crates/trellis-land/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = { workspace = true }
```

`crates/trellis-land/tests/concurrency.rs`:

```rust
//! Many threads land at once on one engine backed by a real SQLite file. Each session writes
//! its own file plus its own key in a shared JSON document, so every landing must succeed
//! (merges are field-level) even though most of them lose the CAS at least once.

use std::collections::BTreeSet;
use std::sync::Arc;

use trellis_core::RepoPath;
use trellis_derive::{DeriveCtx, SkipList};
use trellis_land::{Engine, Outcome, Policy, Route, SystemEnv, WriteMode};
use trellis_store::{FsBlobStore, MetaStore, SqliteMetaStore};

const THREADS: usize = 8;
const SESSIONS_PER_THREAD: usize = 12;

#[test]
fn concurrent_landings_all_succeed_and_form_one_chain() {
    let dir = tempfile::tempdir().unwrap();
    let meta: Arc<dyn MetaStore> = Arc::new(SqliteMetaStore::open(dir.path().join("meta.db")).unwrap());
    let blobs = Arc::new(FsBlobStore::open(dir.path().join("objects")).unwrap());
    let policy = Policy::new(&[("serial/**", WriteMode::Serialize)]).unwrap();
    let engine = Arc::new(Engine::new("repo", Arc::clone(&meta), blobs.clone(), Arc::new(SystemEnv::new()), policy));
    let root = engine.init().unwrap();
    let shared = RepoPath::parse("canon/shared.json").unwrap();

    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let (engine, shared) = (Arc::clone(&engine), shared.clone());
            std::thread::spawn(move || {
                let mut landed = Vec::new();
                for i in 0..SESSIONS_PER_THREAD {
                    let s = engine.open_session(&format!("agent-{t}"), None).unwrap();
                    let current = engine.read(&s.id, &shared).unwrap();
                    let mut doc: serde_json::Map<String, serde_json::Value> = current
                        .map(|b| serde_json::from_slice(&b).unwrap())
                        .unwrap_or_default();
                    doc.insert(format!("t{t}-{i}"), serde_json::json!(i));
                    engine.write(&s.id, &shared, Some(&serde_json::to_vec(&doc).unwrap())).unwrap();
                    let own = RepoPath::parse(&format!("agents/{t}/{i}.txt")).unwrap();
                    engine.write(&s.id, &own, Some(format!("{t}/{i}").as_bytes())).unwrap();
                    if i % 4 == 0 {
                        let serial = RepoPath::parse(&format!("serial/{t}-{i}.txt")).unwrap();
                        engine.write(&s.id, &serial, Some(b"s")).unwrap();
                    }
                    let receipt = engine.land(&s.id, "work", "k").unwrap();
                    match receipt.outcome {
                        Outcome::Landed { changeset, route, .. } => {
                            if i % 4 == 0 {
                                assert_eq!(route, Route::Serialized);
                            }
                            landed.push(changeset);
                        }
                        other => panic!("thread {t} session {i}: {other:?}"),
                    }
                }
                landed
            })
        })
        .collect();
    let landed: BTreeSet<_> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
    assert_eq!(landed.len(), THREADS * SESSIONS_PER_THREAD);

    // Invariant 8: main is one first-parent chain whose generations count every landing.
    let main = engine.main().unwrap();
    let ctx = DeriveCtx::new("repo", meta.as_ref(), blobs.as_ref());
    assert_eq!(ctx.get::<SkipList>(&main).unwrap().generation as usize, landed.len());
    let mut chain = BTreeSet::new();
    let mut current = main;
    while current != root {
        chain.insert(current);
        current = ctx.changeset(&current).unwrap().first_parent().unwrap();
    }
    // Invariant 1: every acknowledged landing is on main.
    assert_eq!(chain, landed);

    // Every field-level JSON merge kept every key.
    let doc: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&engine.read_main(&shared).unwrap().unwrap()).unwrap();
    assert_eq!(doc.len(), THREADS * SESSIONS_PER_THREAD);
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p trellis-land --test concurrency`
Expected: `test result: ok. 1 passed`. If it fails, the failure is a real concurrency bug in Tasks 5–8: stop and debug with superpowers:systematic-debugging before continuing.

- [ ] **Step 3: Commit**

```bash
git add crates/trellis-land
git commit -m "test(land): add a real-thread concurrent landing test"
```

---

### Task 10: `trellis-sim`: deterministic simulation with fault injection

**Files:**
- Create: `crates/trellis-sim/Cargo.toml`, `crates/trellis-sim/src/lib.rs`, `crates/trellis-sim/src/faulty.rs`, `crates/trellis-sim/src/agent.rs`, `crates/trellis-sim/src/check.rs`, `crates/trellis-sim/src/main.rs`

- [ ] **Step 1: Create the crate**

`crates/trellis-sim/Cargo.toml`:

```toml
[package]
name = "trellis-sim"
description = "Deterministic simulation of many agents landing on Trellis, with fault injection and invariant checks."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-core = { workspace = true }
trellis-store = { workspace = true }
trellis-derive = { workspace = true }
trellis-land = { workspace = true }
serde_json = { workspace = true }
rand = { workspace = true }
rand_chacha = { workspace = true }
```

`crates/trellis-sim/src/faulty.rs`:

```rust
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
        Self { inner, rng: Mutex::new(ChaCha8Rng::seed_from_u64(seed)), cas_fault, receipt_fault }
    }

    fn roll(&self, p: f64) -> bool {
        p > 0.0 && self.rng.lock().map(|mut r| r.random_bool(p)).unwrap_or(false)
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
    fn put_derived(&self, repo: &str, cs: &Hash, name: &str, version: u32, value: &[u8]) -> Result<()> {
        self.inner.put_derived(repo, cs, name, version, value)
    }
    fn get_derived(&self, repo: &str, cs: &Hash, name: &str, version: u32) -> Result<Option<Vec<u8>>> {
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
```

`crates/trellis-sim/src/agent.rs`:

```rust
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use trellis_core::RepoPath;
use trellis_land::{Engine, Outcome, Receipt, RebaseOutcome, Resolutions};

/// Files the agents fight over: prose (line merges), a JSON canon (field merges), a strict
/// file (never merged), and a serialized file.
pub const UNIVERSE: [&str; 6] =
    ["notes/a.md", "notes/b.md", "canon/world.json", "src/lib.rs", "strict/rules.md", "serial/Cargo.lock"];

/// One virtual agent's state between steps.
#[derive(Default)]
pub struct Agent {
    pub name: String,
    pub session: Option<String>,
    pub conflict: Option<String>,
    /// A land call that failed with an error; it is retried with the same key.
    pub pending_key: Option<String>,
    /// The last `(session, key)` that got a receipt, re-sent to test idempotency.
    pub last_land: Option<(String, String)>,
    pub next_key: u64,
}

/// What one step produced, for the simulation's bookkeeping.
pub enum Event {
    Nothing,
    Receipt(Receipt),
    Error,
}

fn path(rng: &mut ChaCha8Rng) -> RepoPath {
    RepoPath::parse(UNIVERSE[rng.random_range(0..UNIVERSE.len())]).expect("universe paths are valid")
}

/// New content for `path` derived from what the session currently sees.
fn edit(engine: &Engine, session: &str, path: &RepoPath, agent: &str, rng: &mut ChaCha8Rng) -> Option<Vec<u8>> {
    let current = engine.read(session, path).ok()?;
    if path.as_str().ends_with(".json") {
        let mut doc: serde_json::Map<String, serde_json::Value> =
            current.and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        doc.insert(format!("k{}", rng.random_range(0..8)), serde_json::json!(rng.random_range(0..1000)));
        return serde_json::to_vec_pretty(&doc).ok();
    }
    let mut lines: Vec<String> = match current {
        Some(bytes) => String::from_utf8_lossy(&bytes).lines().map(str::to_owned).collect(),
        None => (0..10).map(|i| format!("line {i}")).collect(),
    };
    if lines.is_empty() {
        lines.push("line 0".into());
    }
    let i = rng.random_range(0..lines.len());
    lines[i] = format!("{agent} edit {}", rng.random_range(0..1000));
    Some(format!("{}\n", lines.join("\n")).into_bytes())
}

impl Agent {
    pub fn step(&mut self, engine: &Engine, rng: &mut ChaCha8Rng) -> Event {
        let Some(session) = self.session.clone() else {
            return match engine.open_session(&self.name, None) {
                Ok(s) => {
                    self.session = Some(s.id);
                    Event::Nothing
                }
                Err(_) => Event::Error,
            };
        };
        if let Some(key) = self.pending_key.clone() {
            return self.land(engine, &session, key);
        }
        if let Some(conflict) = self.conflict.clone() {
            return self.resolve(engine, &conflict, rng);
        }
        match rng.random_range(0..100) {
            0..45 => {
                let p = path(rng);
                if let Some(content) = edit(engine, &session, &p, &self.name, rng) {
                    if engine.write(&session, &p, Some(&content)).is_err() {
                        return Event::Error;
                    }
                }
                Event::Nothing
            }
            45..50 => match engine.write(&session, &path(rng), None) {
                Ok(()) => Event::Nothing,
                Err(_) => Event::Error,
            },
            50..80 => {
                let key = format!("k{}", self.next_key);
                self.next_key += 1;
                self.land(engine, &session, key)
            }
            80..88 => match engine.rebase(&session) {
                Ok(RebaseOutcome::Rebased { .. }) => Event::Nothing,
                Ok(RebaseOutcome::Conflicted { conflict }) => {
                    self.conflict = Some(conflict);
                    Event::Nothing
                }
                Err(_) => Event::Error,
            },
            88..93 => {
                let _ = engine.close_session(&session);
                self.session = None;
                Event::Nothing
            }
            _ => match self.last_land.clone() {
                // A duplicate request: must return the stored receipt, changing nothing.
                Some((session, key)) => match engine.land(&session, "dup", &key) {
                    Ok(receipt) => Event::Receipt(receipt),
                    Err(_) => Event::Nothing,
                },
                None => Event::Nothing,
            },
        }
    }

    fn land(&mut self, engine: &Engine, session: &str, key: String) -> Event {
        match engine.land(session, "sim", &key) {
            Ok(receipt) => {
                self.pending_key = None;
                self.last_land = Some((session.to_owned(), key));
                match &receipt.outcome {
                    Outcome::Landed { .. } | Outcome::Unchanged { .. } => self.session = None,
                    Outcome::Conflicted { conflict } => self.conflict = Some(conflict.clone()),
                    Outcome::Rejected { .. } => {
                        // A strict path moved: rebase onto main and try again later.
                        let _ = engine.rebase(session);
                    }
                }
                Event::Receipt(receipt)
            }
            Err(_) => {
                self.pending_key = Some(key);
                Event::Error
            }
        }
    }

    fn resolve(&mut self, engine: &Engine, conflict: &str, rng: &mut ChaCha8Rng) -> Event {
        let Ok(record) = engine.conflict(conflict) else {
            self.conflict = None;
            return Event::Error;
        };
        let mut resolutions = Resolutions::new();
        for p in &record.paths {
            let pick = match rng.random_range(0..3) {
                0 => p.ours,
                1 => p.theirs,
                _ => None,
            };
            let bytes = match pick {
                Some(blob) => engine.read_blob(&blob.hash).ok(),
                None => None,
            };
            resolutions.insert(p.path.clone(), bytes);
        }
        match engine.resolve_conflict(conflict, &resolutions) {
            Ok(RebaseOutcome::Rebased { .. }) => self.conflict = None,
            Ok(RebaseOutcome::Conflicted { conflict }) => self.conflict = Some(conflict),
            Err(_) => self.conflict = None,
        }
        Event::Nothing
    }
}
```

`crates/trellis-sim/src/check.rs`:

```rust
use std::collections::{BTreeMap, BTreeSet};

use trellis_core::Hash;
use trellis_derive::{DeriveCtx, SkipList};
use trellis_land::{Engine, Outcome, Receipt};

/// Checks spec §7 invariants 1, 3, 6, and 8 over everything the agents saw.
pub fn check(
    engine: &Engine,
    ctx: &DeriveCtx<'_>,
    root: Hash,
    receipts: &BTreeMap<(String, String), Receipt>,
    duplicates_mismatched: usize,
) -> Result<usize, String> {
    // Invariant 3: a repeated request returned exactly the first receipt.
    if duplicates_mismatched > 0 {
        return Err(format!("{duplicates_mismatched} retried requests returned a different receipt"));
    }
    // Invariant 8: main is one first-parent chain with generations 0..n.
    let main = engine.main().map_err(|e| e.to_string())?;
    let mut chain = Vec::new();
    let mut current = main;
    loop {
        let generation = ctx.get::<SkipList>(&current).map_err(|e| e.to_string())?.generation;
        let cs = ctx.changeset(&current).map_err(|e| e.to_string())?;
        chain.push((current, generation, cs.provenance.session.clone()));
        match cs.first_parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    if current != root {
        return Err("main does not descend from the root".into());
    }
    for (i, (_, generation, _)) in chain.iter().rev().enumerate() {
        if *generation != i as u64 {
            return Err(format!("generation {generation} at chain position {i}"));
        }
    }
    // No session landed twice.
    let mut sessions = BTreeSet::new();
    for (_, _, session) in &chain {
        if let Some(session) = session {
            if !sessions.insert(session.clone()) {
                return Err(format!("session {session} landed twice"));
            }
        }
    }
    let on_main: BTreeSet<Hash> = chain.iter().map(|(h, _, _)| *h).collect();
    let mut landed = 0;
    for receipt in receipts.values() {
        match &receipt.outcome {
            // Invariant 1: an acknowledged landing stays on main.
            Outcome::Landed { changeset, .. } => {
                if !on_main.contains(changeset) {
                    return Err(format!("landed changeset {changeset} is not on main"));
                }
                landed += 1;
            }
            // Invariant 6: a conflicted receipt points at a stored conflict.
            Outcome::Conflicted { conflict } => {
                engine.conflict(conflict).map_err(|e| format!("conflict {conflict}: {e}"))?;
            }
            Outcome::Unchanged { .. } | Outcome::Rejected { .. } => {}
        }
    }
    // Every landing on main was acknowledged to its agent (possibly through a retry).
    if landed != chain.len() - 1 {
        return Err(format!("{} changesets on main but {landed} landed receipts", chain.len() - 1));
    }
    Ok(landed)
}
```

`crates/trellis-sim/src/lib.rs`:

```rust
//! Deterministic simulation (spec §8): many virtual agents edit, land, rebase, resolve
//! conflicts, abandon sessions, and repeat requests against one engine, under injected
//! faults. Everything is driven by one seed, so a failure replays exactly.

mod agent;
mod check;
mod faulty;

use std::collections::BTreeMap;
use std::sync::Arc;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use trellis_core::Hash;
use trellis_derive::DeriveCtx;
use trellis_land::{Engine, Outcome, Policy, TestEnv, WriteMode};
use trellis_store::{MemBlobStore, MetaStore, SqliteMetaStore};

pub use agent::{Agent, Event, UNIVERSE};
pub use faulty::FaultyMeta;

#[derive(Clone, Debug)]
pub struct SimConfig {
    pub seed: u64,
    pub agents: usize,
    pub steps: usize,
    /// Probability that a ref CAS spuriously fails.
    pub cas_fault: f64,
    /// Probability that writing a receipt fails after a landing.
    pub receipt_fault: f64,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SimReport {
    pub main: Option<Hash>,
    pub landed: usize,
    pub unchanged: usize,
    pub conflicted: usize,
    pub rejected: usize,
    pub errors: usize,
}

pub fn run(config: &SimConfig) -> Result<SimReport, String> {
    let meta: Arc<dyn MetaStore> = Arc::new(FaultyMeta::new(
        SqliteMetaStore::open_in_memory().map_err(|e| e.to_string())?,
        config.seed ^ 0x5eed,
        config.cas_fault,
        config.receipt_fault,
    ));
    let blobs = Arc::new(MemBlobStore::new());
    let policy = Policy::new(&[("strict/**", WriteMode::Strict), ("serial/**", WriteMode::Serialize)])
        .map_err(|e| e.to_string())?;
    let engine = Engine::new("sim", Arc::clone(&meta), blobs.clone(), Arc::new(TestEnv::new()), policy);
    let root = engine.init().map_err(|e| e.to_string())?;

    let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
    let mut agents: Vec<Agent> =
        (0..config.agents).map(|i| Agent { name: format!("agent-{i}"), ..Default::default() }).collect();
    let mut receipts: BTreeMap<(String, String), trellis_land::Receipt> = BTreeMap::new();
    let mut mismatched = 0;
    let mut report = SimReport::default();

    for _ in 0..config.steps {
        let i = rng.random_range(0..agents.len());
        match agents[i].step(&engine, &mut rng) {
            Event::Nothing => {}
            Event::Error => report.errors += 1,
            Event::Receipt(receipt) => {
                let key = (receipt.session.clone(), receipt.idempotency_key.clone());
                match receipts.get(&key) {
                    Some(first) if *first != receipt => mismatched += 1,
                    Some(_) => {}
                    None => {
                        match &receipt.outcome {
                            Outcome::Landed { .. } => report.landed += 1,
                            Outcome::Unchanged { .. } => report.unchanged += 1,
                            Outcome::Conflicted { .. } => report.conflicted += 1,
                            Outcome::Rejected { .. } => report.rejected += 1,
                        }
                        receipts.insert(key, receipt);
                    }
                }
            }
        }
    }
    // Drain: retry every request that failed with an injected fault, so that every landing
    // on main has been acknowledged before checking.
    for agent in &mut agents {
        for _ in 0..64 {
            let (Some(session), Some(key)) = (agent.session.clone(), agent.pending_key.clone()) else { break };
            if let Ok(receipt) = engine.land(&session, "sim", &key) {
                agent.pending_key = None;
                if let Outcome::Landed { .. } = receipt.outcome {
                    report.landed += 1;
                }
                receipts.insert((receipt.session.clone(), receipt.idempotency_key.clone()), receipt);
            }
        }
    }

    let ctx = DeriveCtx::new("sim", meta.as_ref(), blobs.as_ref());
    check::check(&engine, &ctx, root, &receipts, mismatched)?;
    report.main = Some(engine.main().map_err(|e| e.to_string())?);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(seed: u64) -> SimConfig {
        SimConfig { seed, agents: 12, steps: 1500, cas_fault: 0.15, receipt_fault: 0.05 }
    }

    #[test]
    fn invariants_hold_across_seeds_and_every_route_is_exercised() {
        let mut total = SimReport::default();
        for seed in 0..12 {
            let report = run(&config(seed)).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            total.landed += report.landed;
            total.conflicted += report.conflicted;
            total.rejected += report.rejected;
            total.errors += report.errors;
        }
        assert!(total.landed > 100, "{total:?}");
        assert!(total.conflicted > 0, "{total:?}");
        assert!(total.rejected > 0, "{total:?}");
        assert!(total.errors > 0, "faults were injected: {total:?}");
    }

    #[test]
    fn a_seed_replays_exactly() {
        assert_eq!(run(&config(7)).unwrap(), run(&config(7)).unwrap());
    }
}
```

`crates/trellis-sim/src/main.rs`:

```rust
//! `trellis-sim [seeds] [agents] [steps]`: runs seeds 0..seeds and reports each.

fn main() {
    let args: Vec<u64> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let seeds = args.first().copied().unwrap_or(50);
    let agents = args.get(1).copied().unwrap_or(30) as usize;
    let steps = args.get(2).copied().unwrap_or(5000) as usize;
    let mut failed = 0;
    for seed in 0..seeds {
        let config = trellis_sim::SimConfig { seed, agents, steps, cas_fault: 0.15, receipt_fault: 0.05 };
        match trellis_sim::run(&config) {
            Ok(r) => println!(
                "seed {seed}: ok landed={} conflicted={} rejected={} unchanged={} errors={}",
                r.landed, r.conflicted, r.rejected, r.unchanged, r.errors
            ),
            Err(e) => {
                failed += 1;
                println!("seed {seed}: FAILED {e}");
            }
        }
    }
    if failed > 0 {
        std::process::exit(1);
    }
}
```

- [ ] **Step 2: Run the simulation tests**

Run: `cargo test -p trellis-sim`
Expected: `test result: ok. 2 passed`. A failure prints the seed; replay it with `cargo run -p trellis-sim -- <seeds> 12 1500` and debug with superpowers:systematic-debugging.

- [ ] **Step 3: Run a larger sweep once**

Run: `cargo run --release -p trellis-sim -- 30 30 5000`
Expected: every line ends in `ok`, exit code 0.

- [ ] **Step 4: Commit**

```bash
git add Cargo.lock crates/trellis-sim
git commit -m "feat(sim): add deterministic multi-agent simulation with fault injection"
```

---

### Task 11: Final verification and docs

**Files:**
- Modify: `README.md` (roadmap status)

- [ ] **Step 1: Run every check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: clippy exits 0; every suite reports `ok` (core 15, store 10, merge 16, derive 11, land 23 + 1 integration, sim 2).

- [ ] **Step 2: Update the README status line and roadmap**

In `README.md`, replace the status block:

```markdown
> **Status: design stage.** The design spec is in [`docs/superpowers/specs/2026-10-15-trellis-design.md`](docs/superpowers/specs/2026-10-15-trellis-design.md). Implementation starts with milestone M0. The interfaces below describe the planned v1.
```

with:

```markdown
> **Status: early implementation.** M0 (core, storage, merge, derived data) and M1 (sessions, landing, conflicts, deterministic simulation) are built; the HTTP, MCP, and CLI interfaces below are planned for M3. Design: [`docs/superpowers/specs/2026-10-15-trellis-design.md`](docs/superpowers/specs/2026-10-15-trellis-design.md).
```

and in the Roadmap list, append ` ✅` to the end of the **M0 Core** and **M1 Landing** bullets.

- [ ] **Step 3: Commit**

```bash
git add -A crates README.md
git commit -m "docs: mark M0 and M1 complete"
```

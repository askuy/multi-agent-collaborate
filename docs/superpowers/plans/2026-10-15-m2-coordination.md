# M2: Coordination — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the coordination layer the research says decides collaboration quality: a task queue with atomic first-claim-wins, leases on path scopes, a typed append-only context log with new-only reads, read-set tracking with landing notifications, declared intents, and a YAML policy file — and extend the simulation to check invariants 2, 4, and 5.

**Architecture:** A new `trellis-coord` crate holds coordination state that is independent of versioning: `Scope` (a path, `dir/**`, or `**`), tasks, leases, and the context log, all as records in the metadata store, each mutation under an in-process keyed lock. `Env` and `KeyedLocks` move into it from `trellis-land` (re-exported there). `trellis-land`'s `Engine` owns a `Coord` and uses it for three things: landing is fail-closed when another principal holds an exclusive lease covering a written path (spec §4.2, ATM/AgentRoom); sessions record what they read, and every landing writes a notification for each open session whose reads or watched scopes it touched (spec §4.3, CoAgent "notify, do not lock or abort"); receipts list the session's stale reads. `declare_intent` registers watched scopes and, when exclusive, acquires the leases, which are released when the session lands or closes.

**Tech Stack:** Rust 2024; existing crates plus `serde_yaml` in `trellis-land`.

**Spec:** `docs/superpowers/specs/2026-10-15-trellis-design.md` §2 (Task, Lease, Intent, Notification, Context log), §4.2, §4.3, §4.5, §4.11, §7 invariants 2, 4, 5.

---

## File Structure

```text
crates/trellis-coord/
  Cargo.toml
  src/lib.rs            re-exports
  src/error.rs          CoordError
  src/env.rs            (moved from trellis-land) Env, SystemEnv, TestEnv (+ advance)
  src/locks.rs          (moved from trellis-land, now public) KeyedLocks, Guard
  src/scope.rs          Scope: path | dir/** | **; covers, overlaps; serde as string
  src/coord.rs          Coord: repo, store, env, locks; record helpers
  src/task.rs           Task, TaskStatus, ClaimOutcome; create/claim/release/complete
  src/lease.rs          Lease, LeaseMode, LeaseConflict, Acquire; acquire/renew/release/active/write_blocker
  src/context.rs        Entry, EntryKind, EntryStatus, NewEntry; append/entries/read_new/resolve/open_fails
crates/trellis-land/
  Cargo.toml            + trellis-coord, serde_yaml
  src/lib.rs            re-export Env types from trellis-coord; new exports
  src/error.rs          + Coord variant
  src/records.rs        SessionRecord + reads/watch/leases; Outcome::Landed + stale_reads; Notification
  src/policy.rs         + Policy::from_yaml
  src/engine/mod.rs     Engine owns Coord; read() records reads; sessions(); close releases leases
  src/engine/land.rs    lease check, stale_reads, notify, lease release
  src/engine/notify.rs  notify fan-out, notifications, ack
  src/engine/intent.rs  Intent, IntentOutcome, declare_intent
crates/trellis-sim/
  src/agent.rs          tasks, intents/leases, log, notifications behaviors
  src/check.rs          + leases, tasks, notifications, log-read checks
  src/lib.rs            task pool, per-step checks, report fields
```

---

### Task 1: `trellis-coord` with `Scope`; move `Env` and `KeyedLocks`

**Files:**
- Create: `crates/trellis-coord/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/scope.rs`, `src/coord.rs`
- Move: `crates/trellis-land/src/env.rs` → `crates/trellis-coord/src/env.rs`, `crates/trellis-land/src/locks.rs` → `crates/trellis-coord/src/locks.rs`
- Modify: `Cargo.toml`, `crates/trellis-land/Cargo.toml`, `crates/trellis-land/src/lib.rs`, `crates/trellis-land/src/engine/mod.rs`

- [ ] **Step 1: Move the files and create the crate**

```bash
mkdir -p crates/trellis-coord/src
git mv crates/trellis-land/src/env.rs crates/trellis-coord/src/env.rs
git mv crates/trellis-land/src/locks.rs crates/trellis-coord/src/locks.rs
```

In the root `Cargo.toml` `[workspace.dependencies]`, add `trellis-coord = { path = "crates/trellis-coord" }`.

`crates/trellis-coord/Cargo.toml`:

```toml
[package]
name = "trellis-coord"
description = "Trellis coordination: tasks with atomic claims, leases on path scopes, and the append-only context log."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-core = { workspace = true }
trellis-store = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
parking_lot = { workspace = true }
```

In `crates/trellis-coord/src/locks.rs`, make the API public: change `pub(crate) type Guard` to `pub type Guard`, `pub(crate) struct KeyedLocks` to `pub struct KeyedLocks`, and `pub(crate) fn lock` to `pub fn lock`.

In `crates/trellis-coord/src/env.rs`, add to `impl TestEnv`:

```rust
    /// Moves the clock forward by `ms`.
    pub fn advance(&self, ms: i64) {
        self.clock.fetch_add(ms, Ordering::Relaxed);
    }
```

`crates/trellis-coord/src/lib.rs`:

```rust
//! Coordination (spec §4.2, §4.5): tasks with atomic claims, leases on path scopes, and the
//! append-only context log. Pure coordination state over the metadata store; landing
//! (`trellis-land`) consults it.

mod coord;
mod env;
mod error;
mod locks;
mod scope;

pub use coord::Coord;
pub use env::{Env, SystemEnv, TestEnv};
pub use error::{CoordError, Result};
pub use locks::{Guard, KeyedLocks};
pub use scope::Scope;
```

`crates/trellis-coord/src/error.rs`:

```rust
use trellis_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum CoordError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("record encoding: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("invalid scope {0:?}: use a path, `dir/**`, or `**`")]
    InvalidScope(String),
    #[error("task {0} not found")]
    TaskNotFound(String),
    #[error("{principal} does not own task {task}")]
    NotTaskOwner { task: String, principal: String },
    #[error("lease {0} not found or expired")]
    LeaseNotFound(String),
    #[error("{principal} does not hold lease {lease}")]
    NotLeaseHolder { lease: String, principal: String },
    #[error("log entry {0} not found")]
    EntryNotFound(u64),
}

pub type Result<T> = std::result::Result<T, CoordError>;
```

`crates/trellis-coord/src/coord.rs`:

```rust
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
        Self { repo: repo.into(), meta, env, locks: KeyedLocks::default(), max_claims: 1 }
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
        Ok(self.meta.put_record(&self.repo, kind, id, &serde_json::to_vec(value)?)?)
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
```

`crates/trellis-coord/src/scope.rs`:

```rust
use std::fmt;

use trellis_core::RepoPath;

use crate::error::{CoordError, Result};

/// What a lease or a watch covers: one path, everything under a directory (`dir/**`), or
/// the whole repository (`**`). Overlap is exact and segment-aware, unlike general globs.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Scope {
    All,
    Dir(RepoPath),
    Path(RepoPath),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rp(p: &str) -> RepoPath {
        RepoPath::parse(p).unwrap()
    }

    fn s(x: &str) -> Scope {
        Scope::parse(x).unwrap()
    }

    #[test]
    fn parses_paths_directories_and_everything() {
        assert_eq!(s("**"), Scope::All);
        assert_eq!(s("a/b/**"), Scope::Dir(rp("a/b")));
        assert_eq!(s("a/b.md"), Scope::Path(rp("a/b.md")));
        for bad in ["", "*", "a/*", "a/**/b", "/a", "a/../b", "**/x"] {
            assert!(Scope::parse(bad).is_err(), "{bad:?} should be rejected");
        }
        assert_eq!(s("a/**").to_string(), "a/**");
        assert_eq!(s("**").to_string(), "**");
    }

    #[test]
    fn covers_and_overlaps_respect_segment_boundaries() {
        let dir = s("a/**");
        assert!(dir.covers(&rp("a/x")));
        assert!(dir.covers(&rp("a/x/y")));
        assert!(!dir.covers(&rp("a")));
        assert!(!dir.covers(&rp("ab/x")));
        assert!(Scope::All.covers(&rp("z")));
        assert!(s("a/x").covers(&rp("a/x")));
        assert!(!s("a/x").covers(&rp("a/x/y")));

        assert!(dir.overlaps(&s("a/x")));
        assert!(s("a/x").overlaps(&dir));
        assert!(!dir.overlaps(&s("ab/**")));
        assert!(dir.overlaps(&s("a/b/**")));
        assert!(s("a/b/**").overlaps(&dir));
        assert!(s("a/x").overlaps(&s("a/x")));
        assert!(!s("a/x").overlaps(&s("a/y")));
        assert!(Scope::All.overlaps(&s("q")));
    }

    #[test]
    fn serializes_as_its_string_form() {
        let json = serde_json::to_string(&s("a/**")).unwrap();
        assert_eq!(json, "\"a/**\"");
        assert_eq!(serde_json::from_str::<Scope>(&json).unwrap(), s("a/**"));
        assert!(serde_json::from_str::<Scope>("\"a/*\"").is_err());
    }
}
```

In `crates/trellis-land/Cargo.toml` `[dependencies]`, add `trellis-coord = { workspace = true }` and `serde_yaml = { workspace = true }`. Remove `parking_lot` from it (no longer used directly).

In `crates/trellis-land/src/lib.rs`, delete the lines `mod env;` and `mod locks;`, and replace `pub use env::{Env, SystemEnv, TestEnv};` with `pub use trellis_coord::{Env, SystemEnv, TestEnv};`.

In `crates/trellis-land/src/engine/mod.rs`, replace `use crate::env::Env;` with `use trellis_coord::Env;` and `use crate::locks::{Guard, KeyedLocks};` with `use trellis_coord::{Guard, KeyedLocks};`; in its test module replace `use crate::env::TestEnv;` with `use trellis_coord::TestEnv;`.

- [ ] **Step 2: Run the tests to verify the scope tests fail and the move compiles**

Run: `cargo test -p trellis-coord`
Expected: compile error, `no function or associated item named 'parse' found for enum 'Scope'`.

- [ ] **Step 3: Implement `Scope`**

Insert into `crates/trellis-coord/src/scope.rs` before `#[cfg(test)]`:

```rust
fn is_under(path: &RepoPath, dir: &RepoPath) -> bool {
    path.as_str().strip_prefix(dir.as_str()).is_some_and(|rest| rest.starts_with('/'))
}

impl Scope {
    pub fn parse(s: &str) -> Result<Self> {
        let invalid = || CoordError::InvalidScope(s.to_owned());
        if s == "**" {
            return Ok(Self::All);
        }
        if let Some(dir) = s.strip_suffix("/**") {
            return RepoPath::parse(dir).map(Self::Dir).map_err(|_| invalid());
        }
        if s.contains('*') {
            return Err(invalid());
        }
        RepoPath::parse(s).map(Self::Path).map_err(|_| invalid())
    }

    pub fn covers(&self, path: &RepoPath) -> bool {
        match self {
            Self::All => true,
            Self::Path(p) => p == path,
            Self::Dir(dir) => is_under(path, dir),
        }
    }

    pub fn overlaps(&self, other: &Scope) -> bool {
        match (self, other) {
            (Self::All, _) | (_, Self::All) => true,
            (Self::Path(a), Self::Path(b)) => a == b,
            (Self::Dir(dir), Self::Path(path)) | (Self::Path(path), Self::Dir(dir)) => is_under(path, dir),
            (Self::Dir(a), Self::Dir(b)) => a == b || is_under(a, b) || is_under(b, a),
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => f.write_str("**"),
            Self::Dir(dir) => write!(f, "{dir}/**"),
            Self::Path(path) => write!(f, "{path}"),
        }
    }
}

impl serde::Serialize for Scope {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Scope {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass, including the moved ones and trellis-land**

Run: `cargo test -p trellis-coord && cargo test -p trellis-land`
Expected: trellis-coord `test result: ok. 6 passed` (3 scope + 2 env + 1 locks); trellis-land `test result: ok. 20 passed` (23 minus the 3 moved tests) and the concurrency test passes.

- [ ] **Step 5: Commit**

```bash
git add -A Cargo.toml Cargo.lock crates/trellis-coord crates/trellis-land
git commit -m "feat(coord): add trellis-coord with Scope; move Env and KeyedLocks into it"
```

---

### Task 2: Tasks with atomic first-claim-wins

**Files:**
- Create: `crates/trellis-coord/src/task.rs`
- Modify: `crates/trellis-coord/src/lib.rs`

- [ ] **Step 1: Write the types and failing tests**

`crates/trellis-coord/src/task.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::coord::Coord;
use crate::error::{CoordError, Result};
use crate::scope::Scope;

pub(crate) const TASK: &str = "task";
const TASKS_LOCK: &str = "tasks";

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum TaskStatus {
    Open,
    Claimed { owner: String, at_ms: i64 },
    Done { owner: String, summary: String },
}

/// One unit of work in the shared queue (spec §2, DeLM task queue).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    /// Intended output, inputs, and the interface shared with other work (DeLM's guidance).
    pub detail: String,
    pub created_by: String,
    pub scope: Vec<Scope>,
    pub status: TaskStatus,
    pub created_at_ms: i64,
}

/// The deterministic answer to a claim (AgentRoom: claims return a conflict, never a guess).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ClaimOutcome {
    Claimed(Task),
    Taken { owner: String },
    AtLimit { holding: Vec<String> },
    Finished,
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use trellis_store::SqliteMetaStore;

    use super::*;
    use crate::env::TestEnv;

    pub(crate) fn coord() -> (Coord, Arc<TestEnv>) {
        let env = Arc::new(TestEnv::new());
        let coord = Coord::new("r", Arc::new(SqliteMetaStore::open_in_memory().unwrap()), env.clone());
        (coord, env)
    }

    #[test]
    fn the_first_claim_wins_and_an_agent_holds_one_claim() {
        let (c, _) = coord();
        let t1 = c.create_task("planner", "ch1", "write chapter 1", vec![]).unwrap();
        let t2 = c.create_task("planner", "ch2", "write chapter 2", vec![]).unwrap();
        assert!(matches!(c.claim_task(&t1.id, "a").unwrap(), ClaimOutcome::Claimed(_)));
        assert_eq!(c.claim_task(&t1.id, "b").unwrap(), ClaimOutcome::Taken { owner: "a".into() });
        assert!(matches!(c.claim_task(&t1.id, "a").unwrap(), ClaimOutcome::Claimed(_)));
        assert_eq!(c.claim_task(&t2.id, "a").unwrap(), ClaimOutcome::AtLimit { holding: vec![t1.id.clone()] });
        assert!(matches!(c.claim_task(&t2.id, "b").unwrap(), ClaimOutcome::Claimed(_)));
    }

    #[test]
    fn only_the_owner_completes_or_releases() {
        let (c, _) = coord();
        let t = c.create_task("planner", "x", "", vec![]).unwrap();
        c.claim_task(&t.id, "a").unwrap();
        assert!(matches!(c.complete_task(&t.id, "b", "nope"), Err(CoordError::NotTaskOwner { .. })));
        assert_eq!(c.release_task(&t.id, "a").unwrap().status, TaskStatus::Open);
        c.claim_task(&t.id, "b").unwrap();
        let done = c.complete_task(&t.id, "b", "wrote it").unwrap();
        assert_eq!(done.status, TaskStatus::Done { owner: "b".into(), summary: "wrote it".into() });
        assert_eq!(c.claim_task(&t.id, "c").unwrap(), ClaimOutcome::Finished);
        assert!(matches!(c.task("missing"), Err(CoordError::TaskNotFound(_))));
        assert_eq!(c.tasks().unwrap().len(), 1);
    }

    #[test]
    fn the_claim_limit_is_configurable() {
        let (c, _) = coord();
        let c = c.with_max_claims(2);
        let t1 = c.create_task("p", "1", "", vec![]).unwrap();
        let t2 = c.create_task("p", "2", "", vec![]).unwrap();
        let t3 = c.create_task("p", "3", "", vec![]).unwrap();
        assert!(matches!(c.claim_task(&t1.id, "a").unwrap(), ClaimOutcome::Claimed(_)));
        assert!(matches!(c.claim_task(&t2.id, "a").unwrap(), ClaimOutcome::Claimed(_)));
        assert!(matches!(c.claim_task(&t3.id, "a").unwrap(), ClaimOutcome::AtLimit { .. }));
    }

    #[test]
    fn concurrent_claims_have_one_winner() {
        let (c, _) = coord();
        let c = Arc::new(c);
        let t = c.create_task("p", "contested", "", vec![]).unwrap();
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let (c, id) = (Arc::clone(&c), t.id.clone());
                std::thread::spawn(move || c.claim_task(&id, &format!("agent-{i}")).unwrap())
            })
            .collect();
        let winners = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|o| matches!(o, ClaimOutcome::Claimed(_)))
            .count();
        assert_eq!(winners, 1);
    }
}
```

In `crates/trellis-coord/src/lib.rs`, add `mod task;` (after `mod scope;`) and `pub use task::{ClaimOutcome, Task, TaskStatus};`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-coord task`
Expected: compile error, `no method named 'create_task' found for struct 'Coord'`.

- [ ] **Step 3: Implement the task operations**

Insert into `crates/trellis-coord/src/task.rs` before `#[cfg(test)]`:

```rust
impl Coord {
    pub fn create_task(&self, created_by: &str, subject: &str, detail: &str, scope: Vec<Scope>) -> Result<Task> {
        let task = Task {
            id: self.env.new_id("t"),
            subject: subject.to_owned(),
            detail: detail.to_owned(),
            created_by: created_by.to_owned(),
            scope,
            status: TaskStatus::Open,
            created_at_ms: self.env.now_ms(),
        };
        self.save(TASK, &task.id, &task)?;
        Ok(task)
    }

    pub fn task(&self, id: &str) -> Result<Task> {
        self.load(TASK, id)?.ok_or_else(|| CoordError::TaskNotFound(id.to_owned()))
    }

    /// Every task, oldest first.
    pub fn tasks(&self) -> Result<Vec<Task>> {
        let mut tasks: Vec<Task> = self.load_all(TASK)?;
        tasks.sort_by(|a, b| (a.created_at_ms, &a.id).cmp(&(b.created_at_ms, &b.id)));
        Ok(tasks)
    }

    /// The first claim wins; a principal holds at most `max_claims` unfinished claims.
    /// Re-claiming a task one already owns is idempotent.
    pub fn claim_task(&self, id: &str, principal: &str) -> Result<ClaimOutcome> {
        let _guard = self.locks.lock(&[TASKS_LOCK.to_owned()]);
        let mut task = self.task(id)?;
        match &task.status {
            TaskStatus::Done { .. } => return Ok(ClaimOutcome::Finished),
            TaskStatus::Claimed { owner, .. } if owner == principal => return Ok(ClaimOutcome::Claimed(task)),
            TaskStatus::Claimed { owner, .. } => return Ok(ClaimOutcome::Taken { owner: owner.clone() }),
            TaskStatus::Open => {}
        }
        let holding: Vec<String> = self
            .tasks()?
            .into_iter()
            .filter(|t| matches!(&t.status, TaskStatus::Claimed { owner, .. } if owner == principal))
            .map(|t| t.id)
            .collect();
        if holding.len() >= self.max_claims {
            return Ok(ClaimOutcome::AtLimit { holding });
        }
        task.status = TaskStatus::Claimed { owner: principal.to_owned(), at_ms: self.env.now_ms() };
        self.save(TASK, &task.id, &task)?;
        Ok(ClaimOutcome::Claimed(task))
    }

    fn owned_task(&self, id: &str, principal: &str) -> Result<Task> {
        let task = self.task(id)?;
        match &task.status {
            TaskStatus::Claimed { owner, .. } if owner == principal => Ok(task),
            _ => Err(CoordError::NotTaskOwner { task: id.to_owned(), principal: principal.to_owned() }),
        }
    }

    /// Gives a claimed task back to the queue.
    pub fn release_task(&self, id: &str, principal: &str) -> Result<Task> {
        let _guard = self.locks.lock(&[TASKS_LOCK.to_owned()]);
        let mut task = self.owned_task(id, principal)?;
        task.status = TaskStatus::Open;
        self.save(TASK, &task.id, &task)?;
        Ok(task)
    }

    pub fn complete_task(&self, id: &str, principal: &str, summary: &str) -> Result<Task> {
        let _guard = self.locks.lock(&[TASKS_LOCK.to_owned()]);
        let mut task = self.owned_task(id, principal)?;
        task.status = TaskStatus::Done { owner: principal.to_owned(), summary: summary.to_owned() };
        self.save(TASK, &task.id, &task)?;
        Ok(task)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-coord`
Expected: `test result: ok. 10 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-coord
git commit -m "feat(coord): add the task queue with atomic first-claim-wins"
```

---

### Task 3: Leases

**Files:**
- Create: `crates/trellis-coord/src/lease.rs`
- Modify: `crates/trellis-coord/src/lib.rs`

- [ ] **Step 1: Write the types and failing tests**

`crates/trellis-coord/src/lease.rs`:

```rust
use serde::{Deserialize, Serialize};
use trellis_core::RepoPath;

use crate::coord::Coord;
use crate::error::{CoordError, Result};
use crate::scope::Scope;

pub(crate) const LEASE: &str = "lease";
const LEASES_LOCK: &str = "leases";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LeaseMode {
    Exclusive,
    Shared,
}

/// A claim on a scope until `expires_at_ms` (spec §2, AgentRoom claims, Agent Mail leases).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Lease {
    pub id: String,
    pub holder: String,
    pub scope: Scope,
    pub mode: LeaseMode,
    pub intent: String,
    pub expires_at_ms: i64,
}

/// Who is in the way, what for, and until when.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LeaseConflict {
    pub holder: String,
    pub scope: Scope,
    pub mode: LeaseMode,
    pub intent: String,
    pub expires_at_ms: i64,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Acquire {
    Granted(Vec<Lease>),
    Conflict(LeaseConflict),
}

impl From<&Lease> for LeaseConflict {
    fn from(lease: &Lease) -> Self {
        Self {
            holder: lease.holder.clone(),
            scope: lease.scope.clone(),
            mode: lease.mode,
            intent: lease.intent.clone(),
            expires_at_ms: lease.expires_at_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::tests::coord;

    fn s(x: &str) -> Scope {
        Scope::parse(x).unwrap()
    }

    fn rp(x: &str) -> RepoPath {
        RepoPath::parse(x).unwrap()
    }

    fn granted(a: Acquire) -> Vec<Lease> {
        match a {
            Acquire::Granted(leases) => leases,
            Acquire::Conflict(c) => panic!("expected a grant, got {c:?}"),
        }
    }

    #[test]
    fn exclusive_leases_exclude_overlapping_scopes_of_other_holders() {
        let (c, _) = coord();
        let held = granted(c.acquire_leases("a", &[s("book/ch1/**")], LeaseMode::Exclusive, 100, "ch1").unwrap());
        let Acquire::Conflict(conflict) =
            c.acquire_leases("b", &[s("book/ch1/s1.md")], LeaseMode::Shared, 100, "peek").unwrap()
        else {
            panic!("expected a conflict")
        };
        assert_eq!((conflict.holder.as_str(), conflict.intent.as_str()), ("a", "ch1"));
        granted(c.acquire_leases("b", &[s("book/ch2/**")], LeaseMode::Exclusive, 100, "ch2").unwrap());

        // All or nothing: one blocked scope grants none.
        let all = c.acquire_leases("c", &[s("free.md"), s("book/**")], LeaseMode::Exclusive, 100, "x").unwrap();
        assert!(matches!(all, Acquire::Conflict(_)));
        assert!(c.active_leases().unwrap().iter().all(|l| l.holder != "c"));

        assert_eq!(c.write_blocker(&rp("book/ch1/s1.md"), "b").unwrap().unwrap().holder, "a");
        assert_eq!(c.write_blocker(&rp("book/ch1/s1.md"), "a").unwrap(), None);
        assert_eq!(c.write_blocker(&rp("elsewhere.md"), "b").unwrap(), None);

        c.release_lease(&held[0].id, "a").unwrap();
        assert_eq!(c.write_blocker(&rp("book/ch1/s1.md"), "b").unwrap(), None);
    }

    #[test]
    fn shared_leases_coexist_and_a_holder_never_blocks_itself() {
        let (c, _) = coord();
        granted(c.acquire_leases("a", &[s("notes/**")], LeaseMode::Shared, 100, "read").unwrap());
        granted(c.acquire_leases("b", &[s("notes/x.md")], LeaseMode::Shared, 100, "read").unwrap());
        granted(c.acquire_leases("a", &[s("notes/x.md")], LeaseMode::Shared, 100, "more").unwrap());
        let excl = c.acquire_leases("b", &[s("notes/**")], LeaseMode::Exclusive, 100, "w").unwrap();
        assert!(matches!(excl, Acquire::Conflict(_)));
        granted(c.acquire_leases("z", &[s("solo/**")], LeaseMode::Exclusive, 100, "w").unwrap());
        granted(c.acquire_leases("z", &[s("solo/a.md")], LeaseMode::Exclusive, 100, "w2").unwrap());
    }

    #[test]
    fn leases_expire_and_can_be_renewed_by_their_holder() {
        let (c, env) = coord();
        let held = granted(c.acquire_leases("a", &[s("x")], LeaseMode::Exclusive, 50, "w").unwrap());
        env.advance(30);
        let renewed = c.renew_lease(&held[0].id, "a", 50).unwrap();
        assert!(renewed.expires_at_ms > held[0].expires_at_ms);
        assert!(matches!(c.renew_lease(&held[0].id, "b", 50), Err(CoordError::NotLeaseHolder { .. })));
        assert!(matches!(c.release_lease(&held[0].id, "b"), Err(CoordError::NotLeaseHolder { .. })));
        env.advance(1000);
        assert!(c.active_leases().unwrap().is_empty());
        assert!(matches!(c.renew_lease(&held[0].id, "a", 50), Err(CoordError::LeaseNotFound(_))));
        granted(c.acquire_leases("b", &[s("x")], LeaseMode::Exclusive, 50, "after").unwrap());
        c.release_lease("never-existed", "b").unwrap();
    }
}
```

In `crates/trellis-coord/src/lib.rs`, add `mod lease;` and `pub use lease::{Acquire, Lease, LeaseConflict, LeaseMode};`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-coord lease`
Expected: compile error, `no method named 'acquire_leases' found for struct 'Coord'`.

- [ ] **Step 3: Implement the lease operations**

Insert into `crates/trellis-coord/src/lease.rs` before `#[cfg(test)]`:

```rust
fn clashes(existing: &Lease, holder: &str, scope: &Scope, mode: LeaseMode) -> bool {
    existing.holder != holder
        && existing.scope.overlaps(scope)
        && (existing.mode == LeaseMode::Exclusive || mode == LeaseMode::Exclusive)
}

impl Coord {
    /// Grants a lease per scope, all or nothing; a conflict names the blocking lease.
    /// Expired leases are pruned on the way.
    pub fn acquire_leases(
        &self,
        holder: &str,
        scopes: &[Scope],
        mode: LeaseMode,
        ttl_ms: i64,
        intent: &str,
    ) -> Result<Acquire> {
        let _guard = self.locks.lock(&[LEASES_LOCK.to_owned()]);
        let now = self.env.now_ms();
        let mut active = Vec::new();
        for lease in self.load_all::<Lease>(LEASE)? {
            if lease.expires_at_ms > now {
                active.push(lease);
            } else {
                self.delete(LEASE, &lease.id)?;
            }
        }
        for scope in scopes {
            if let Some(blocking) = active.iter().find(|l| clashes(l, holder, scope, mode)) {
                return Ok(Acquire::Conflict(blocking.into()));
            }
        }
        let leases: Vec<Lease> = scopes
            .iter()
            .map(|scope| Lease {
                id: self.env.new_id("l"),
                holder: holder.to_owned(),
                scope: scope.clone(),
                mode,
                intent: intent.to_owned(),
                expires_at_ms: now + ttl_ms,
            })
            .collect();
        for lease in &leases {
            self.save(LEASE, &lease.id, lease)?;
        }
        Ok(Acquire::Granted(leases))
    }

    pub fn renew_lease(&self, id: &str, holder: &str, ttl_ms: i64) -> Result<Lease> {
        let _guard = self.locks.lock(&[LEASES_LOCK.to_owned()]);
        let now = self.env.now_ms();
        let mut lease: Lease = self
            .load(LEASE, id)?
            .filter(|l: &Lease| l.expires_at_ms > now)
            .ok_or_else(|| CoordError::LeaseNotFound(id.to_owned()))?;
        if lease.holder != holder {
            return Err(CoordError::NotLeaseHolder { lease: id.to_owned(), principal: holder.to_owned() });
        }
        lease.expires_at_ms = now + ttl_ms;
        self.save(LEASE, id, &lease)?;
        Ok(lease)
    }

    /// Releases a lease; releasing one that is gone is not an error.
    pub fn release_lease(&self, id: &str, holder: &str) -> Result<()> {
        let _guard = self.locks.lock(&[LEASES_LOCK.to_owned()]);
        match self.load::<Lease>(LEASE, id)? {
            None => Ok(()),
            Some(lease) if lease.holder != holder => {
                Err(CoordError::NotLeaseHolder { lease: id.to_owned(), principal: holder.to_owned() })
            }
            Some(_) => self.delete(LEASE, id),
        }
    }

    pub fn active_leases(&self) -> Result<Vec<Lease>> {
        let now = self.env.now_ms();
        Ok(self.load_all::<Lease>(LEASE)?.into_iter().filter(|l| l.expires_at_ms > now).collect())
    }

    /// The active exclusive lease, held by someone other than `principal`, that covers `path`.
    pub fn write_blocker(&self, path: &RepoPath, principal: &str) -> Result<Option<LeaseConflict>> {
        Ok(self
            .active_leases()?
            .iter()
            .find(|l| l.holder != principal && l.mode == LeaseMode::Exclusive && l.scope.covers(path))
            .map(LeaseConflict::from))
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-coord`
Expected: `test result: ok. 13 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-coord
git commit -m "feat(coord): add leases on path scopes with expiry and all-or-nothing grants"
```

---

### Task 4: The context log

**Files:**
- Create: `crates/trellis-coord/src/context.rs`
- Modify: `crates/trellis-coord/src/lib.rs`

- [ ] **Step 1: Write the types and failing tests**

`crates/trellis-coord/src/context.rs`:

```rust
use serde::{Deserialize, Serialize};
use trellis_core::RepoPath;

use crate::coord::Coord;
use crate::error::{CoordError, Result};
use crate::scope::Scope;

const ENTRY: &str = "log";
const LOG_META: &str = "log-meta";
const CURSOR: &str = "log-cursor";
const LOG_LOCK: &str = "log";

/// The kinds of shared-context entries (spec §4.5, DeLM FACT/FAIL/summary).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    /// A finding supported by evidence.
    Fact,
    /// A contradicted approach or a defect in someone's result; open until resolved.
    Fail,
    /// The result of a finished task.
    Summary,
    /// An agreed decision.
    Decision,
    Note,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryStatus {
    Open,
    Resolved,
}

/// What an author supplies; the log assigns the sequence number.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct NewEntry {
    pub kind: Option<EntryKind>,
    pub topics: Vec<String>,
    pub scope: Vec<Scope>,
    pub body: String,
    pub evidence: Vec<String>,
}

/// An immutable log entry; only `status` and `resolution` change, when a FAIL is resolved.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub kind: EntryKind,
    pub author: String,
    pub topics: Vec<String>,
    pub scope: Vec<Scope>,
    pub body: String,
    pub evidence: Vec<String>,
    pub status: EntryStatus,
    pub resolution: Option<String>,
    pub created_at_ms: i64,
}

fn entry_id(seq: u64) -> String {
    format!("{seq:020}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::tests::coord;

    fn entry(kind: EntryKind, topic: &str, body: &str) -> NewEntry {
        NewEntry { kind: Some(kind), topics: vec![topic.into()], body: body.into(), ..Default::default() }
    }

    fn seqs(entries: &[Entry]) -> Vec<u64> {
        entries.iter().map(|e| e.seq).collect()
    }

    #[test]
    fn reads_return_only_entries_not_seen_before() {
        let (c, _) = coord();
        let first = c.append("a", entry(EntryKind::Fact, "ch1", "alice is injured")).unwrap();
        assert_eq!((first.seq, first.status), (1, EntryStatus::Resolved));
        c.append("b", entry(EntryKind::Fail, "ch2", "timeline broken")).unwrap();
        assert_eq!(seqs(&c.read_new("reader", &[]).unwrap()), [1, 2]);
        assert!(c.read_new("reader", &[]).unwrap().is_empty());
        c.append("a", entry(EntryKind::Note, "ch1", "later")).unwrap();
        assert_eq!(seqs(&c.read_new("reader", &[]).unwrap()), [3]);
        // Another reader has its own cursor.
        assert_eq!(seqs(&c.read_new("other", &[]).unwrap()), [1, 2, 3]);
    }

    #[test]
    fn topic_filters_skip_entries_without_redelivering_them() {
        let (c, _) = coord();
        c.append("a", entry(EntryKind::Fact, "ch1", "x")).unwrap();
        c.append("a", entry(EntryKind::Fact, "ch2", "y")).unwrap();
        let only_ch1 = c.read_new("r", &["ch1".into()]).unwrap();
        assert_eq!(seqs(&only_ch1), [1]);
        assert!(c.read_new("r", &[]).unwrap().is_empty());
        assert_eq!(seqs(&c.entries(0, 10).unwrap()), [1, 2]);
        assert_eq!(seqs(&c.entries(1, 10).unwrap()), [2]);
        assert_eq!(seqs(&c.entries(0, 1).unwrap()), [1]);
    }

    #[test]
    fn fails_stay_open_until_resolved_and_filter_by_scope() {
        let (c, _) = coord();
        let fail = c
            .append(
                "checker",
                NewEntry {
                    kind: Some(EntryKind::Fail),
                    scope: vec![Scope::parse("book/ch1/**").unwrap()],
                    body: "alice lifts with an injured arm".into(),
                    evidence: vec!["changeset abc".into()],
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(fail.status, EntryStatus::Open);
        let inside = RepoPath::parse("book/ch1/s2.md").unwrap();
        let outside = RepoPath::parse("book/ch2/s1.md").unwrap();
        assert_eq!(c.open_fails(Some(&inside)).unwrap().len(), 1);
        assert!(c.open_fails(Some(&outside)).unwrap().is_empty());
        assert_eq!(c.open_fails(None).unwrap().len(), 1);
        let resolved = c.resolve_entry(fail.seq, "writer", "fixed in s2").unwrap();
        assert_eq!(resolved.status, EntryStatus::Resolved);
        assert_eq!(resolved.resolution.as_deref(), Some("writer: fixed in s2"));
        assert!(c.open_fails(None).unwrap().is_empty());
        assert!(matches!(c.resolve_entry(99, "x", "y"), Err(CoordError::EntryNotFound(99))));
    }
}
```

In `crates/trellis-coord/src/lib.rs`, add `mod context;` and `pub use context::{Entry, EntryKind, EntryStatus, NewEntry};`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-coord context`
Expected: compile error, `no method named 'append' found for struct 'Coord'`.

- [ ] **Step 3: Implement the log**

Insert into `crates/trellis-coord/src/context.rs` before `#[cfg(test)]`:

```rust
impl Coord {
    /// Appends an entry with the next sequence number. FAIL entries start open.
    pub fn append(&self, author: &str, new: NewEntry) -> Result<Entry> {
        let _guard = self.locks.lock(&[LOG_LOCK.to_owned()]);
        let seq = self.load::<u64>(LOG_META, "seq")?.unwrap_or(0) + 1;
        let kind = new.kind.unwrap_or(EntryKind::Note);
        let entry = Entry {
            seq,
            kind,
            author: author.to_owned(),
            topics: new.topics,
            scope: new.scope,
            body: new.body,
            evidence: new.evidence,
            status: if kind == EntryKind::Fail { EntryStatus::Open } else { EntryStatus::Resolved },
            resolution: None,
            created_at_ms: self.env.now_ms(),
        };
        self.save(ENTRY, &entry_id(seq), &entry)?;
        self.save(LOG_META, "seq", &seq)?;
        Ok(entry)
    }

    /// Entries after `after_seq`, oldest first, at most `limit`.
    pub fn entries(&self, after_seq: u64, limit: usize) -> Result<Vec<Entry>> {
        Ok(self
            .load_all::<Entry>(ENTRY)?
            .into_iter()
            .filter(|e| e.seq > after_seq)
            .take(limit)
            .collect())
    }

    /// What `reader` has not seen yet, filtered by `topics` (empty means every entry). The
    /// cursor moves past everything new, including filtered-out entries, so nothing is ever
    /// delivered twice (DeLM: a read shows only what is new since the last read).
    pub fn read_new(&self, reader: &str, topics: &[String]) -> Result<Vec<Entry>> {
        let _guard = self.locks.lock(&[format!("cursor:{reader}")]);
        let cursor: u64 = self.load(CURSOR, reader)?.unwrap_or(0);
        let fresh = self.entries(cursor, usize::MAX)?;
        if let Some(last) = fresh.last() {
            self.save(CURSOR, reader, &last.seq)?;
        }
        Ok(fresh
            .into_iter()
            .filter(|e| topics.is_empty() || e.topics.iter().any(|t| topics.contains(t)))
            .collect())
    }

    pub fn resolve_entry(&self, seq: u64, by: &str, note: &str) -> Result<Entry> {
        let _guard = self.locks.lock(&[LOG_LOCK.to_owned()]);
        let mut entry: Entry = self.load(ENTRY, &entry_id(seq))?.ok_or(CoordError::EntryNotFound(seq))?;
        entry.status = EntryStatus::Resolved;
        entry.resolution = Some(format!("{by}: {note}"));
        self.save(ENTRY, &entry_id(seq), &entry)?;
        Ok(entry)
    }

    /// Open FAIL entries; with `path`, only those whose scope covers it (or that have no scope).
    pub fn open_fails(&self, path: Option<&RepoPath>) -> Result<Vec<Entry>> {
        Ok(self
            .entries(0, usize::MAX)?
            .into_iter()
            .filter(|e| e.kind == EntryKind::Fail && e.status == EntryStatus::Open)
            .filter(|e| path.is_none_or(|p| e.scope.is_empty() || e.scope.iter().any(|s| s.covers(p))))
            .collect())
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-coord`
Expected: `test result: ok. 16 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-coord
git commit -m "feat(coord): add the typed context log with new-only reads"
```

---

### Task 5: Read tracking, notifications, and stale reads in `trellis-land`

**Files:**
- Modify: `crates/trellis-land/src/records.rs`, `src/error.rs`, `src/lib.rs`, `src/engine/mod.rs`, `src/engine/land.rs`
- Create: `crates/trellis-land/src/engine/notify.rs`

- [ ] **Step 1: Extend the records**

In `crates/trellis-land/src/records.rs`:

Add `use std::collections::BTreeSet;` (next to the `BTreeMap` import) and `use trellis_coord::Scope;`.

Add after `pub(crate) const CONFLICT: &str = "conflict";`:

```rust
pub(crate) const NOTE_PREFIX: &str = "note:";
```

Add three fields at the end of `SessionRecord` (after `created_at_ms`):

```rust
    /// Paths read from the base; a landing that changes one notifies this session.
    #[serde(default)]
    pub reads: BTreeSet<RepoPath>,
    /// Scopes declared in intents; also trigger notifications.
    #[serde(default)]
    pub watch: Vec<Scope>,
    /// Leases acquired through intents, released when the session lands or closes.
    #[serde(default)]
    pub leases: Vec<String>,
```

Change `Outcome::Landed` to:

```rust
    Landed {
        changeset: Hash,
        seq: u64,
        route: Route,
        /// Paths this session read that changed on `main` after its base (spec §4.3).
        #[serde(default)]
        stale_reads: Vec<RepoPath>,
    },
```

Add after `ConflictRecord`:

```rust
/// A landing that touched something a session read or watches (spec §4.3).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Notification {
    pub changeset: Hash,
    pub seq: u64,
    pub author: String,
    pub author_session: Option<String>,
    pub message: String,
    /// The changed paths this session read or watches.
    pub paths: Vec<RepoPath>,
    pub acked: bool,
}
```

In the records test, add `stale_reads: vec![]` to the `Outcome::Landed { .. }` literal, and add `reads: Default::default(), watch: vec![], leases: vec![],` to the `SessionRecord` literal.

In `crates/trellis-land/src/lib.rs`, add `Notification` to the `pub use records::{...}` list.

- [ ] **Step 2: Update existing code for the new fields**

In `crates/trellis-land/src/engine/mod.rs` `open_session`, add to the `SessionRecord` literal:

```rust
            reads: Default::default(),
            watch: Vec::new(),
            leases: Vec::new(),
```

In `crates/trellis-land/src/engine/land.rs`, change the `landed` test helper's pattern to `Outcome::Landed { changeset, seq, route, .. } => (*changeset, *seq, *route),`, and in `land_locked` give both `Outcome::Landed` constructions a `stale_reads` field: the recovery path uses `stale_reads: Vec::new()` (the original receipt was never delivered, so there is nothing to stay consistent with), and the CAS-success path uses:

```rust
                let stale_reads = session.reads.intersection(&scan.touched).cloned().collect();
                return Ok(Outcome::Landed { changeset: id, seq, route, stale_reads });
```

- [ ] **Step 3: Write the failing tests**

`crates/trellis-land/src/engine/notify.rs`:

```rust
use trellis_core::{Hash, RepoPath};
use trellis_derive::{SkipList, is_first_parent_ancestor};

use super::Engine;
use crate::error::Result;
use crate::records::{NOTE_PREFIX, Notification, SessionRecord, decode, encode};

fn note_kind(session: &str) -> String {
    format!("{NOTE_PREFIX}{session}")
}

fn watches(session: &SessionRecord, path: &RepoPath) -> bool {
    session.reads.contains(path) || session.watch.iter().any(|scope| scope.covers(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::{engine, p, seed};
    use crate::records::Outcome;

    #[test]
    fn sessions_that_read_a_changed_path_are_notified() {
        let e = engine();
        seed(&e, "doc.md", b"v1");
        let reader = e.open_session("reader", None).unwrap();
        let bystander = e.open_session("bystander", None).unwrap();
        e.read(&reader.id, &p("doc.md")).unwrap();
        e.read(&bystander.id, &p("other.md")).unwrap();
        let writer = e.open_session("writer", None).unwrap();
        e.read(&writer.id, &p("doc.md")).unwrap();
        e.write(&writer.id, &p("doc.md"), Some(b"v2")).unwrap();
        let Outcome::Landed { changeset, .. } = e.land(&writer.id, "update doc", "k").unwrap().outcome else {
            panic!("expected a landing")
        };

        let notes = e.notifications(&reader.id).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].changeset, changeset);
        assert_eq!(notes[0].paths, vec![p("doc.md")]);
        assert_eq!(notes[0].author, "writer");
        assert_eq!(notes[0].message, "update doc");
        assert!(e.notifications(&bystander.id).unwrap().is_empty());
        assert!(e.notifications(&writer.id).unwrap().is_empty());

        assert_eq!(e.ack_notifications(&reader.id, notes[0].seq).unwrap(), 1);
        assert!(e.notifications(&reader.id).unwrap().is_empty());
        assert_eq!(e.all_notifications(&reader.id).unwrap().len(), 1);
    }

    #[test]
    fn sessions_that_already_include_a_change_are_not_notified() {
        let e = engine();
        seed(&e, "doc.md", b"v1");
        let writer = e.open_session("writer", None).unwrap();
        e.write(&writer.id, &p("doc.md"), Some(b"v2")).unwrap();
        e.land(&writer.id, "w", "k").unwrap();
        let later = e.open_session("later", None).unwrap();
        e.read(&later.id, &p("doc.md")).unwrap();
        e.notify(&e.main().unwrap()).unwrap();
        assert!(e.notifications(&later.id).unwrap().is_empty());
    }

    #[test]
    fn stale_reads_are_reported_in_the_receipt() {
        let e = engine();
        seed(&e, "doc.md", b"v1");
        let s = e.open_session("s", None).unwrap();
        e.read(&s.id, &p("doc.md")).unwrap();
        e.read(&s.id, &p("quiet.md")).unwrap();
        let other = e.open_session("other", None).unwrap();
        e.write(&other.id, &p("doc.md"), Some(b"v2")).unwrap();
        e.land(&other.id, "o", "k").unwrap();
        e.write(&s.id, &p("mine.txt"), Some(b"m")).unwrap();
        let Outcome::Landed { stale_reads, .. } = e.land(&s.id, "s", "k").unwrap().outcome else {
            panic!("expected a landing")
        };
        assert_eq!(stale_reads, vec![p("doc.md")]);
    }
}
```

In `crates/trellis-land/src/engine/mod.rs`, add `mod notify;` with the other engine modules, and add to its test module:

```rust
    pub(crate) fn seed(e: &Engine, path: &str, content: &[u8]) {
        let s = e.open_session("seed", None).unwrap();
        e.write(&s.id, &p(path), Some(content)).unwrap();
        assert!(matches!(e.land(&s.id, "seed", "k").unwrap().outcome, crate::Outcome::Landed { .. }));
    }
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p trellis-land notify`
Expected: compile error, `no method named 'notifications' found for struct 'Engine'`.

- [ ] **Step 5: Implement read tracking, session listing, and notifications**

In `crates/trellis-land/src/engine/mod.rs`, replace `read` with:

```rust
    /// Reads `path` as the session sees it: its own writes over its base. Reads from the
    /// base are recorded, so a later landing that changes the path notifies this session.
    pub fn read(&self, session: &str, path: &RepoPath) -> Result<Option<Vec<u8>>> {
        let _guard = self.lock_session(session);
        let mut record = self.session(session)?;
        match record.overlay.get(path) {
            Some(FileChange::Upsert { blob, .. }) => Ok(Some(self.read_blob(&blob.hash)?)),
            Some(FileChange::Delete) => Ok(None),
            None => {
                let content = read_file(&self.ctx(), &record.base, path)?;
                if record.status == SessionStatus::Open && record.reads.insert(path.clone()) {
                    self.save_session(&record)?;
                }
                Ok(content)
            }
        }
    }

    /// Every session, in id order; `open_only` keeps only open ones.
    pub fn sessions(&self, open_only: bool) -> Result<Vec<SessionRecord>> {
        let mut out = Vec::new();
        for (_, bytes) in self.meta.list_records(&self.repo, SESSION)? {
            let record: SessionRecord = decode(&bytes)?;
            if !open_only || record.status == SessionStatus::Open {
                out.push(record);
            }
        }
        Ok(out)
    }
```

Insert into `crates/trellis-land/src/engine/notify.rs` before `#[cfg(test)]`:

```rust
impl Engine {
    /// Records a notification for every open session that read or watches a path
    /// `changeset` changed and whose base does not include it yet (spec §4.3, CoAgent:
    /// notify, do not lock or abort). Idempotent: re-running keeps existing notifications.
    pub(crate) fn notify(&self, changeset: &Hash) -> Result<()> {
        let ctx = self.ctx();
        let cs = ctx.changeset(changeset)?;
        let seq = ctx.get::<SkipList>(changeset)?.generation;
        for session in self.sessions(true)? {
            if cs.provenance.session.as_deref() == Some(session.id.as_str())
                || is_first_parent_ancestor(&ctx, changeset, &session.base)?
            {
                continue;
            }
            let paths: Vec<RepoPath> = cs.changes.keys().filter(|p| watches(&session, p)).cloned().collect();
            if paths.is_empty() {
                continue;
            }
            let kind = note_kind(&session.id);
            let id = format!("{seq:020}");
            if self.meta.get_record(&self.repo, &kind, &id)?.is_none() {
                let note = Notification {
                    changeset: *changeset,
                    seq,
                    author: cs.provenance.principal.clone(),
                    author_session: cs.provenance.session.clone(),
                    message: cs.message.clone(),
                    paths,
                    acked: false,
                };
                self.meta.put_record(&self.repo, &kind, &id, &encode(&note)?)?;
            }
        }
        Ok(())
    }

    /// Every notification for `session`, oldest first, acknowledged or not.
    pub fn all_notifications(&self, session: &str) -> Result<Vec<Notification>> {
        self.meta
            .list_records(&self.repo, &note_kind(session))?
            .into_iter()
            .map(|(_, bytes)| decode(&bytes))
            .collect()
    }

    /// Unacknowledged notifications for `session`, oldest first.
    pub fn notifications(&self, session: &str) -> Result<Vec<Notification>> {
        Ok(self.all_notifications(session)?.into_iter().filter(|n| !n.acked).collect())
    }

    /// Acknowledges every notification up to and including `through_seq`; returns how many.
    pub fn ack_notifications(&self, session: &str, through_seq: u64) -> Result<usize> {
        let kind = note_kind(session);
        let mut acked = 0;
        for mut note in self.notifications(session)? {
            if note.seq <= through_seq {
                note.acked = true;
                self.meta.put_record(&self.repo, &kind, &format!("{:020}", note.seq), &encode(&note)?)?;
                acked += 1;
            }
        }
        Ok(acked)
    }
}
```

In `crates/trellis-land/src/engine/land.rs` `land`, right after `let outcome = self.land_locked(...)?;`, add:

```rust
        if let Outcome::Landed { changeset, .. } = &outcome {
            self.notify(changeset)?;
        }
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p trellis-land`
Expected: unit tests `test result: ok. 23 passed`; the concurrency test passes.

- [ ] **Step 7: Commit**

```bash
git add crates/trellis-land
git commit -m "feat(land): track reads, notify sessions on landings, and report stale reads"
```

---

### Task 6: Intents and lease enforcement

**Files:**
- Create: `crates/trellis-land/src/engine/intent.rs`
- Modify: `crates/trellis-land/src/error.rs`, `src/lib.rs`, `src/engine/mod.rs`, `src/engine/land.rs`

- [ ] **Step 1: Give the engine a `Coord`**

In `crates/trellis-land/src/error.rs`, add the variant (after `Core`):

```rust
    #[error(transparent)]
    Coord(#[from] trellis_coord::CoordError),
```

In `crates/trellis-land/src/engine/mod.rs`, add `use trellis_coord::Coord;`, add a field `coord: Coord,` to `Engine`, and change `Engine::new` to:

```rust
    pub fn new(
        repo: impl Into<String>,
        meta: Arc<dyn MetaStore>,
        blobs: Arc<dyn BlobStore>,
        env: Arc<dyn Env>,
        policy: Policy,
    ) -> Self {
        let repo = repo.into();
        let coord = Coord::new(repo.clone(), Arc::clone(&meta), Arc::clone(&env));
        Self { repo, meta, blobs, env, policy, locks: KeyedLocks::default(), coord }
    }

    /// Tasks, leases, and the context log for this repository.
    pub fn coord(&self) -> &Coord {
        &self.coord
    }

    pub(crate) fn release_session_leases(&self, session: &SessionRecord) -> Result<()> {
        for lease in &session.leases {
            self.coord.release_lease(lease, &session.principal)?;
        }
        Ok(())
    }
```

In `close_session`, after `self.save_session(&record)` succeeds, release the leases; replace its last line with:

```rust
        self.save_session(&record)?;
        self.release_session_leases(&record)
```

Add `mod intent;` with the other engine modules and `pub use intent::{Intent, IntentOutcome};` next to `pub use conflict::RebaseOutcome;`. In `crates/trellis-land/src/lib.rs` change the engine re-export to `pub use engine::{Engine, Intent, IntentOutcome, MAIN, MAX_LAND_ATTEMPTS, RebaseOutcome};`.

- [ ] **Step 2: Write the intent type and failing tests**

`crates/trellis-land/src/engine/intent.rs`:

```rust
use serde::{Deserialize, Serialize};
use trellis_coord::{Acquire, Lease, LeaseConflict, LeaseMode, Scope};

use super::Engine;
use crate::error::Result;

/// A session's declared footprint (spec §2, CoAgent footprints, ATM write intents).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Intent {
    pub description: String,
    /// Scopes the session depends on; landings there notify it.
    pub reads: Vec<Scope>,
    /// Scopes the session will write; also watched.
    pub writes: Vec<Scope>,
    /// Take exclusive leases on `writes`: other principals' landings there are rejected.
    pub exclusive: bool,
    pub ttl_ms: i64,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IntentOutcome {
    Admitted { leases: Vec<Lease> },
    Conflict(LeaseConflict),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::{engine, p};
    use crate::records::Outcome;

    fn scope(s: &str) -> Scope {
        Scope::parse(s).unwrap()
    }

    #[test]
    fn exclusive_intents_reject_other_principals_landings_until_released() {
        let e = engine();
        let a = e.open_session("alice", None).unwrap();
        let b = e.open_session("bob", None).unwrap();
        let mine = Intent {
            description: "rewrite chapter 3".into(),
            writes: vec![scope("book/ch3/**")],
            exclusive: true,
            ttl_ms: 60_000,
            ..Default::default()
        };
        let IntentOutcome::Admitted { leases } = e.declare_intent(&a.id, &mine).unwrap() else {
            panic!("expected admission")
        };
        assert_eq!(leases.len(), 1);

        let theirs = Intent { writes: vec![scope("book/ch3/s1.md")], exclusive: true, ttl_ms: 1000, ..Default::default() };
        let IntentOutcome::Conflict(conflict) = e.declare_intent(&b.id, &theirs).unwrap() else {
            panic!("expected a conflict")
        };
        assert_eq!((conflict.holder.as_str(), conflict.intent.as_str()), ("alice", "rewrite chapter 3"));

        // Leases are advisory for writing, enforced at landing (fail-closed).
        e.write(&b.id, &p("book/ch3/s1.md"), Some(b"bob")).unwrap();
        let Outcome::Rejected { reason } = e.land(&b.id, "bob", "k").unwrap().outcome else {
            panic!("expected a rejection")
        };
        assert!(reason.contains("alice"), "{reason}");

        e.write(&a.id, &p("book/ch3/s1.md"), Some(b"alice")).unwrap();
        assert!(matches!(e.land(&a.id, "alice", "k").unwrap().outcome, Outcome::Landed { .. }));
        assert!(e.coord().active_leases().unwrap().is_empty());
        assert!(!matches!(e.land(&b.id, "bob", "k2").unwrap().outcome, Outcome::Rejected { .. }));
    }

    #[test]
    fn intents_watch_scopes_for_notifications() {
        let e = engine();
        let watcher = e.open_session("watcher", None).unwrap();
        let watch = Intent { reads: vec![scope("canon/**")], ..Default::default() };
        assert!(matches!(e.declare_intent(&watcher.id, &watch).unwrap(), IntentOutcome::Admitted { .. }));
        let w = e.open_session("w", None).unwrap();
        e.write(&w.id, &p("canon/world.json"), Some(b"{}")).unwrap();
        e.land(&w.id, "canon", "k").unwrap();
        assert_eq!(e.notifications(&watcher.id).unwrap().len(), 1);
    }

    #[test]
    fn closing_a_session_releases_its_leases() {
        let e = engine();
        let a = e.open_session("alice", None).unwrap();
        let intent = Intent { writes: vec![scope("x/**")], exclusive: true, ttl_ms: 60_000, ..Default::default() };
        e.declare_intent(&a.id, &intent).unwrap();
        assert_eq!(e.coord().active_leases().unwrap().len(), 1);
        e.close_session(&a.id).unwrap();
        assert!(e.coord().active_leases().unwrap().is_empty());
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p trellis-land intent`
Expected: compile error, `no method named 'declare_intent' found for struct 'Engine'`.

- [ ] **Step 4: Implement intents, lease enforcement, and lease release**

Insert into `crates/trellis-land/src/engine/intent.rs` before `#[cfg(test)]`:

```rust
impl Engine {
    /// Registers the session's footprint: its read and write scopes are watched, and with
    /// `exclusive` the write scopes are leased to the session's principal (all or nothing).
    pub fn declare_intent(&self, session: &str, intent: &Intent) -> Result<IntentOutcome> {
        let _guard = self.lock_session(session);
        let mut record = self.open_session_record(session)?;
        let leases = if intent.exclusive && !intent.writes.is_empty() {
            match self.coord.acquire_leases(
                &record.principal,
                &intent.writes,
                LeaseMode::Exclusive,
                intent.ttl_ms,
                &intent.description,
            )? {
                Acquire::Granted(leases) => leases,
                Acquire::Conflict(conflict) => return Ok(IntentOutcome::Conflict(conflict)),
            }
        } else {
            Vec::new()
        };
        record.watch.extend(intent.reads.iter().cloned());
        record.watch.extend(intent.writes.iter().cloned());
        record.watch.sort();
        record.watch.dedup();
        record.leases.extend(leases.iter().map(|l| l.id.clone()));
        self.save_session(&record)?;
        Ok(IntentOutcome::Admitted { leases })
    }
}
```

In `crates/trellis-land/src/engine/land.rs` `land_locked`, insert at the top of the function body (before `let ctx = self.ctx();`):

```rust
        for path in session.overlay.keys() {
            if let Some(blocker) = self.coord.write_blocker(path, &session.principal)? {
                return Ok(Outcome::Rejected {
                    reason: format!(
                        "{path} is leased exclusively by {} until {} ({})",
                        blocker.holder, blocker.expires_at_ms, blocker.intent
                    ),
                });
            }
        }
```

In `land`, after the session status update (the `if let Some(status) = status { ... }` block), release the session's leases when it is no longer open. Replace that block with:

```rust
        if let Some(status) = status {
            let mut record = record;
            record.status = status;
            record.overlay.clear();
            self.save_session(&record)?;
            self.release_session_leases(&record)?;
        }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p trellis-land`
Expected: unit tests `test result: ok. 26 passed`; the concurrency test passes.

- [ ] **Step 6: Commit**

```bash
git add crates/trellis-land
git commit -m "feat(land): add intents, exclusive-lease enforcement at landing, and lease release"
```

---

### Task 7: Policy file

**Files:**
- Modify: `crates/trellis-land/src/policy.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `crates/trellis-land/src/policy.rs`:

```rust
    #[test]
    fn loads_rules_from_a_yaml_policy_file() {
        let policy = Policy::from_yaml(
            "rules:\n  - pattern: \"canon/**\"\n    mode: strict\n  - pattern: \"**/*.lock\"\n    mode: serialize\n",
        )
        .unwrap();
        assert_eq!(policy.mode(&path("canon/a.json")), WriteMode::Strict);
        assert_eq!(policy.mode(&path("x/Cargo.lock")), WriteMode::Serialize);
        assert_eq!(policy.mode(&path("src/a.rs")), WriteMode::Optimistic);
        assert!(Policy::from_yaml("rules: [{pattern: \"a\", mode: sometimes}]").is_err());
        assert_eq!(Policy::from_yaml("").unwrap().mode(&path("a")), WriteMode::Optimistic);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p trellis-land policy`
Expected: compile error, `no function or associated item named 'from_yaml'`.

- [ ] **Step 3: Implement `from_yaml`**

Insert into `crates/trellis-land/src/policy.rs` before `#[cfg(test)]`:

```rust
#[derive(Deserialize, Default)]
struct PolicyFile {
    #[serde(default)]
    rules: Vec<RuleFile>,
}

#[derive(Deserialize)]
struct RuleFile {
    pattern: String,
    mode: WriteMode,
}

impl Policy {
    /// Parses a `trellis.policy.yaml` (spec §4.11):
    ///
    /// ```yaml
    /// rules:
    ///   - pattern: "canon/**"
    ///     mode: strict
    /// ```
    pub fn from_yaml(text: &str) -> Result<Self> {
        let file: PolicyFile = if text.trim().is_empty() {
            PolicyFile::default()
        } else {
            serde_yaml::from_str(text).map_err(|e| LandError::Policy {
                pattern: "<policy file>".into(),
                message: e.to_string(),
            })?
        };
        let rules: Vec<(&str, WriteMode)> = file.rules.iter().map(|r| (r.pattern.as_str(), r.mode)).collect();
        Self::new(&rules)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-land`
Expected: unit tests `test result: ok. 27 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-land
git commit -m "feat(land): load write policy from a YAML file"
```

---

### Task 8: Simulation: tasks, leases, the log, notifications; invariants 2, 4, 5

**Files:**
- Modify: `crates/trellis-sim/Cargo.toml`, `src/agent.rs`, `src/check.rs`, `src/lib.rs`

- [ ] **Step 1: Add the dependency**

In `crates/trellis-sim/Cargo.toml` `[dependencies]`, add `trellis-coord = { workspace = true }`.

- [ ] **Step 2: Replace the agent**

Replace `crates/trellis-sim/src/agent.rs` with:

```rust
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use trellis_coord::{ClaimOutcome, EntryKind, NewEntry, Scope, TaskStatus};
use trellis_core::RepoPath;
use trellis_land::{Engine, Intent, Outcome, RebaseOutcome, Receipt, Resolutions};

/// Files the agents fight over: prose (line merges), a JSON canon (field merges), a strict
/// file (never merged), and a serialized file.
pub const UNIVERSE: [&str; 6] =
    ["notes/a.md", "notes/b.md", "canon/world.json", "src/lib.rs", "strict/rules.md", "serial/Cargo.lock"];
const SCOPES: [&str; 5] = ["notes/**", "notes/a.md", "canon/**", "src/lib.rs", "serial/**"];
const TOPICS: [&str; 3] = ["notes", "canon", "src"];

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
    pub task: Option<String>,
    pub last_seen_seq: u64,
}

/// What one step produced, for the simulation's bookkeeping and checks.
pub enum Event {
    Nothing,
    Receipt { receipt: Receipt, first_try: bool },
    Read { previous: u64, seqs: Vec<u64> },
    Error,
}

fn path(rng: &mut ChaCha8Rng) -> RepoPath {
    RepoPath::parse(UNIVERSE[rng.random_range(0..UNIVERSE.len())]).expect("universe paths are valid")
}

fn scope(rng: &mut ChaCha8Rng) -> Scope {
    Scope::parse(SCOPES[rng.random_range(0..SCOPES.len())]).expect("scopes are valid")
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
            return self.start(engine, rng);
        };
        if let Some(key) = self.pending_key.clone() {
            return self.land(engine, &session, key);
        }
        if let Some(conflict) = self.conflict.clone() {
            return self.resolve(engine, &conflict, rng);
        }
        let coord = engine.coord();
        match rng.random_range(0..100) {
            0..38 => {
                let p = path(rng);
                if let Some(content) = edit(engine, &session, &p, &self.name, rng) {
                    if engine.write(&session, &p, Some(&content)).is_err() {
                        return Event::Error;
                    }
                }
                Event::Nothing
            }
            38..42 => match engine.write(&session, &path(rng), None) {
                Ok(()) => Event::Nothing,
                Err(_) => Event::Error,
            },
            42..68 => {
                let key = format!("k{}", self.next_key);
                self.next_key += 1;
                self.land(engine, &session, key)
            }
            68..75 => match engine.rebase(&session) {
                Ok(RebaseOutcome::Rebased { .. }) => Event::Nothing,
                Ok(RebaseOutcome::Conflicted { conflict }) => {
                    self.conflict = Some(conflict);
                    Event::Nothing
                }
                Err(_) => Event::Error,
            },
            75..79 => {
                let _ = engine.close_session(&session);
                self.session = None;
                if let Some(task) = self.task.take() {
                    let _ = coord.release_task(&task, &self.name);
                }
                Event::Nothing
            }
            79..84 => match self.last_land.clone() {
                // A duplicate request: must return the stored receipt, changing nothing.
                Some((session, key)) => match engine.land(&session, "dup", &key) {
                    Ok(receipt) => Event::Receipt { receipt, first_try: false },
                    Err(_) => Event::Nothing,
                },
                None => Event::Nothing,
            },
            84..89 => {
                let kind = if rng.random_bool(0.3) { EntryKind::Fail } else { EntryKind::Fact };
                let entry = NewEntry {
                    kind: Some(kind),
                    topics: vec![TOPICS[rng.random_range(0..TOPICS.len())].to_owned()],
                    body: format!("{} observation {}", self.name, rng.random_range(0..1000)),
                    ..Default::default()
                };
                match coord.append(&self.name, entry) {
                    Ok(_) => Event::Nothing,
                    Err(_) => Event::Error,
                }
            }
            89..93 => {
                let topics: Vec<String> =
                    if rng.random_bool(0.5) { Vec::new() } else { vec![TOPICS[rng.random_range(0..TOPICS.len())].to_owned()] };
                match coord.read_new(&self.name, &topics) {
                    Ok(entries) => {
                        let previous = self.last_seen_seq;
                        let seqs: Vec<u64> = entries.iter().map(|e| e.seq).collect();
                        if let Some(last) = seqs.last() {
                            self.last_seen_seq = *last;
                        }
                        Event::Read { previous, seqs }
                    }
                    Err(_) => Event::Error,
                }
            }
            93..96 => {
                if let Ok(notes) = engine.notifications(&session) {
                    if let Some(last) = notes.last() {
                        let _ = engine.ack_notifications(&session, last.seq);
                    }
                }
                Event::Nothing
            }
            96..98 => {
                if let Ok(fails) = coord.open_fails(None) {
                    if let Some(fail) = fails.first() {
                        let _ = coord.resolve_entry(fail.seq, &self.name, "handled");
                    }
                }
                Event::Nothing
            }
            _ => {
                let watch = Intent { description: "watch".into(), reads: vec![scope(rng)], ..Default::default() };
                match engine.declare_intent(&session, &watch) {
                    Ok(_) => Event::Nothing,
                    Err(_) => Event::Error,
                }
            }
        }
    }

    /// Optionally claims a task, opens a session, and optionally leases a scope.
    fn start(&mut self, engine: &Engine, rng: &mut ChaCha8Rng) -> Event {
        let coord = engine.coord();
        if self.task.is_none() && rng.random_bool(0.5) {
            if let Ok(tasks) = coord.tasks() {
                let open: Vec<_> = tasks.into_iter().filter(|t| t.status == TaskStatus::Open).collect();
                if !open.is_empty() {
                    let id = open[rng.random_range(0..open.len())].id.clone();
                    if let Ok(ClaimOutcome::Claimed(task)) = coord.claim_task(&id, &self.name) {
                        self.task = Some(task.id);
                    }
                }
            }
        }
        let session = match engine.open_session(&self.name, self.task.as_deref()) {
            Ok(s) => s.id,
            Err(_) => return Event::Error,
        };
        self.session = Some(session.clone());
        if rng.random_bool(0.3) {
            let intent = Intent {
                description: format!("{} owns this for now", self.name),
                writes: vec![scope(rng)],
                exclusive: true,
                ttl_ms: 300,
                ..Default::default()
            };
            if engine.declare_intent(&session, &intent).is_err() {
                return Event::Error;
            }
        }
        Event::Nothing
    }

    fn land(&mut self, engine: &Engine, session: &str, key: String) -> Event {
        let first_try = self.pending_key.is_none();
        match engine.land(session, "sim", &key) {
            Ok(receipt) => {
                self.pending_key = None;
                self.last_land = Some((session.to_owned(), key));
                match &receipt.outcome {
                    Outcome::Landed { .. } | Outcome::Unchanged { .. } => {
                        self.session = None;
                        if let Some(task) = self.task.take() {
                            let _ = engine.coord().complete_task(&task, &self.name, "landed");
                        }
                    }
                    Outcome::Conflicted { conflict } => self.conflict = Some(conflict.clone()),
                    Outcome::Rejected { .. } => {
                        // A strict path moved or a lease blocks: rebase and try again later.
                        let _ = engine.rebase(session);
                    }
                }
                Event::Receipt { receipt, first_try }
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

- [ ] **Step 3: Add the new checks**

In `crates/trellis-sim/src/check.rs`, add these imports at the top:

```rust
use trellis_coord::{LeaseMode, TaskStatus};
use trellis_derive::is_first_parent_ancestor;
```

Then append:

```rust
/// Invariant 2: no two active leases of different holders overlap unless both are shared.
pub fn check_leases(engine: &Engine) -> Result<(), String> {
    let leases = engine.coord().active_leases().map_err(|e| e.to_string())?;
    for (i, a) in leases.iter().enumerate() {
        for b in &leases[i + 1..] {
            let exclusive = a.mode == LeaseMode::Exclusive || b.mode == LeaseMode::Exclusive;
            if a.holder != b.holder && exclusive && a.scope.overlaps(&b.scope) {
                return Err(format!("leases {} ({}) and {} ({}) overlap", a.id, a.holder, b.id, b.holder));
            }
        }
    }
    Ok(())
}

/// Invariant 4: no principal holds more than one unfinished claim.
pub fn check_tasks(engine: &Engine) -> Result<(), String> {
    let mut owners = BTreeSet::new();
    for task in engine.coord().tasks().map_err(|e| e.to_string())? {
        if let TaskStatus::Claimed { owner, .. } = task.status {
            if !owners.insert(owner.clone()) {
                return Err(format!("{owner} holds more than one claim"));
            }
        }
    }
    Ok(())
}

/// Invariant 5: a landing notified every open session that read or watches a path it
/// changed and does not include it yet.
pub fn check_notified(engine: &Engine, ctx: &DeriveCtx<'_>, receipt: &Receipt) -> Result<(), String> {
    let Outcome::Landed { changeset, .. } = &receipt.outcome else { return Ok(()) };
    let cs = ctx.changeset(changeset).map_err(|e| e.to_string())?;
    for session in engine.sessions(true).map_err(|e| e.to_string())? {
        if session.id == receipt.session
            || is_first_parent_ancestor(ctx, changeset, &session.base).map_err(|e| e.to_string())?
        {
            continue;
        }
        let affected = cs
            .changes
            .keys()
            .any(|p| session.reads.contains(p) || session.watch.iter().any(|w| w.covers(p)));
        let notified = engine
            .all_notifications(&session.id)
            .map_err(|e| e.to_string())?
            .iter()
            .any(|n| n.changeset == *changeset);
        if affected && !notified {
            return Err(format!("session {} was not notified of {changeset}", session.id));
        }
    }
    Ok(())
}

/// A log read returns ascending entries, all newer than anything that reader saw before.
pub fn check_read(previous: u64, seqs: &[u64]) -> Result<(), String> {
    if seqs.windows(2).any(|w| w[0] >= w[1]) || seqs.first().is_some_and(|first| *first <= previous) {
        return Err(format!("log read after {previous} returned {seqs:?}"));
    }
    Ok(())
}
```

- [ ] **Step 4: Wire the checks and the task pool into the run loop**

In `crates/trellis-sim/src/lib.rs`:

Add fields to `SimReport` (after `errors`):

```rust
    pub tasks_done: usize,
    pub notifications: usize,
    pub log_entries: u64,
```

After `let root = engine.init().map_err(|e| e.to_string())?;`, create the task pool and a derive context for per-step checks:

```rust
    for i in 0..24 {
        engine
            .coord()
            .create_task("planner", &format!("task {i}"), "simulated work", Vec::new())
            .map_err(|e| e.to_string())?;
    }
    let ctx = DeriveCtx::new("sim", meta.as_ref(), blobs.as_ref());
```

Replace the step loop's `match` with:

```rust
        match agents[i].step(&engine, &mut rng) {
            Event::Nothing => {}
            Event::Error => report.errors += 1,
            Event::Read { previous, seqs } => check::check_read(previous, &seqs)?,
            Event::Receipt { receipt, first_try } => {
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
                        if first_try {
                            check::check_notified(&engine, &ctx, &receipt)?;
                        }
                        receipts.insert(key, receipt);
                    }
                }
            }
        }
        check::check_leases(&engine)?;
        check::check_tasks(&engine)?;
```

Delete the later line `let ctx = DeriveCtx::new("sim", meta.as_ref(), blobs.as_ref());` (the context now exists earlier), and before `report.main = ...` add:

```rust
    report.tasks_done = engine
        .coord()
        .tasks()
        .map_err(|e| e.to_string())?
        .iter()
        .filter(|t| matches!(t.status, trellis_coord::TaskStatus::Done { .. }))
        .count();
    for session in engine.sessions(false).map_err(|e| e.to_string())? {
        report.notifications += engine.all_notifications(&session.id).map_err(|e| e.to_string())?.len();
    }
    report.log_entries = engine.coord().entries(0, usize::MAX).map_err(|e| e.to_string())?.len() as u64;
```

In `crates/trellis-sim/src/main.rs`, extend the success line to also print `tasks_done={} notifications={} log={}` with `r.tasks_done, r.notifications, r.log_entries`.

In the `invariants_hold_across_seeds_and_every_route_is_exercised` test, also sum `tasks_done` and `notifications` into `total` and add:

```rust
        assert!(total.tasks_done > 0, "{total:?}");
        assert!(total.notifications > 0, "{total:?}");
```

- [ ] **Step 5: Run the simulation tests and a sweep**

Run: `cargo test -p trellis-sim && cargo run --release -p trellis-sim -- 30 30 5000`
Expected: `test result: ok. 2 passed`; every sweep line ends in `ok` and the command exits 0. A failure names the seed and the violated invariant; debug with superpowers:systematic-debugging.

- [ ] **Step 6: Commit**

```bash
git add Cargo.lock crates/trellis-sim
git commit -m "feat(sim): simulate tasks, leases, the log, and notifications; check invariants 2, 4, 5"
```

---

### Task 9: Final verification and docs

- [ ] **Step 1: Run every check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: clippy exits 0; all suites `ok` (core 15, store 10, merge 16, derive 11, coord 16, land 27 + 1 integration, sim 2).

- [ ] **Step 2: Update the README**

In `README.md`, change the status sentence to say M0, M1, and M2 (tasks, leases, intents, notifications, the context log, policy files) are built, and append ` ✅` to the **M2 Coordination** roadmap bullet.

- [ ] **Step 3: Commit**

```bash
git add -A crates README.md docs/superpowers/plans/2026-10-15-m2-coordination.md
git commit -m "docs: mark M2 complete"
```

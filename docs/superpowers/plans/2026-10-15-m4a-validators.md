# M4a: Validators — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run checks after every landing (spec §4.7; Scion's post-merge testing instead of a merge queue; Atlas-style consistency checks) and turn failures into FAIL log entries and repair tasks that agents pick up.

**Architecture:** A new `trellis-validate` crate. Validators are declared in the `validators:` section of `trellis.policy.yaml` (name, scopes, command, timeout, severity). A `Runner` owns a working directory per repository and a position file (the last validated sequence number). On first use it materializes current `main` and starts there (history is not re-validated); afterwards it applies each newly landed changeset to the directory in order (incremental, so build caches stay warm) and runs every validator whose scopes cover a changed path, with `TRELLIS_CHANGESET`, `TRELLIS_SEQ`, `TRELLIS_PRINCIPAL`, and `TRELLIS_CHANGED` in the environment. A non-zero exit, a timeout, or a failure to start is a finding: severity `error` appends a FAIL entry (topics `validator` and the validator's name, scoped to the changed paths, with the changeset as evidence) and creates a repair task; severity `warning` appends a NOTE only. The server runs the runner for every active repository on a background thread.

**Tech Stack:** `std::process` (timeouts by polling `try_wait`), `serde_yaml`.

**Spec:** §4.7, §4.11 (policy file).

---

### Task 1: Files at a changeset

**Files:** Modify `crates/trellis-land/src/engine/mod.rs`

- [ ] **Step 1: Write the failing test**

Add to the engine's test module:

```rust
    #[test]
    fn files_at_lists_mains_tree_at_a_changeset() {
        let e = engine();
        let first = {
            seed(&e, "a.md", b"1");
            e.main().unwrap()
        };
        seed(&e, "b.md", b"2");
        let paths = |id: &Hash| -> Vec<String> {
            e.files_at(id).unwrap().iter().map(|f| f.path.to_string()).collect()
        };
        assert_eq!(paths(&first), ["a.md"]);
        assert_eq!(paths(&e.main().unwrap()), ["a.md", "b.md"]);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p trellis-land files_at`
Expected: compile error, `no method named 'files_at'`.

- [ ] **Step 3: Implement**

Add to `impl Engine` (next to `session_files`):

```rust
    /// Every file of the tree at `changeset`, in path order.
    pub fn files_at(&self, changeset: &Hash) -> Result<Vec<FileEntry>> {
        let ctx = self.ctx();
        let root = ctx.get::<TreeIndex>(changeset)?.root;
        Ok(flatten(&ctx, &root)?
            .into_iter()
            .map(|(path, (blob, kind))| FileEntry { path, blob, kind, changed: false })
            .collect())
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p trellis-land`
Expected: unit tests `test result: ok. 32 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-land
git commit -m "feat(land): list the files of main at a changeset"
```

---

### Task 2: `trellis-validate`

**Files:**
- Modify: `Cargo.toml` (add `trellis-validate = { path = "crates/trellis-validate" }`)
- Create: `crates/trellis-validate/Cargo.toml`, `src/lib.rs`, `src/spec.rs`, `src/runner.rs`, `tests/runner.rs`

- [ ] **Step 1: Create the crate**

`crates/trellis-validate/Cargo.toml`:

```toml
[package]
name = "trellis-validate"
description = "Trellis validation after landing: run checks on each landed changeset; failures become FAIL entries and repair tasks."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-core = { workspace = true }
trellis-coord = { workspace = true }
trellis-land = { workspace = true }
serde = { workspace = true }
serde_yaml = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
trellis-store = { workspace = true }
tempfile = { workspace = true }
```

`crates/trellis-validate/src/lib.rs`:

```rust
//! Validation after landing (spec §4.7): instead of a merge queue, run checks on each
//! landed changeset (Scion's post-merge testing) and turn failures into FAIL log entries and
//! repair tasks that agents pick up.

mod runner;
mod spec;

pub use runner::{Finding, RunError, Runner};
pub use spec::{Severity, SpecError, ValidatorSpec, from_yaml};
```

`crates/trellis-validate/src/spec.rs`:

```rust
use serde::{Deserialize, Serialize};
use trellis_coord::Scope;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// A FAIL entry and a repair task.
    #[default]
    Error,
    /// A NOTE entry only.
    Warning,
}

/// One check run after landings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ValidatorSpec {
    pub name: String,
    /// Run when a landing changes a path in one of these scopes (default: everything).
    #[serde(default = "everything")]
    pub scope: Vec<Scope>,
    /// Program and arguments, run in a checkout of `main` at the landed changeset.
    pub command: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub severity: Severity,
}

fn everything() -> Vec<Scope> {
    vec![Scope::All]
}

fn default_timeout() -> u64 {
    600
}

#[derive(Debug, thiserror::Error)]
#[error("invalid validators: {0}")]
pub struct SpecError(pub String);

#[derive(Deserialize, Default)]
struct PolicyFile {
    #[serde(default)]
    validators: Vec<ValidatorSpec>,
}

/// Reads the `validators:` section of a `trellis.policy.yaml`; other sections are ignored.
pub fn from_yaml(text: &str) -> Result<Vec<ValidatorSpec>, SpecError> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let file: PolicyFile = serde_yaml::from_str(text).map_err(|e| SpecError(e.to_string()))?;
    for validator in &file.validators {
        if validator.command.is_empty() {
            return Err(SpecError(format!("validator {} has an empty command", validator.name)));
        }
    }
    Ok(file.validators)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_validators_section_of_a_policy_file() {
        let text = r#"
rules:
  - pattern: "canon/**"
    mode: strict
validators:
  - name: tests
    scope: ["src/**"]
    command: ["cargo", "test"]
    timeout_seconds: 900
  - name: spelling
    command: ["sh", "-c", "true"]
    severity: warning
"#;
        let validators = from_yaml(text).unwrap();
        assert_eq!(validators.len(), 2);
        assert_eq!(validators[0].scope, vec![Scope::parse("src/**").unwrap()]);
        assert_eq!(validators[0].timeout_seconds, 900);
        assert_eq!(validators[0].severity, Severity::Error);
        assert_eq!(validators[1].scope, vec![Scope::All]);
        assert_eq!(validators[1].timeout_seconds, 600);
        assert_eq!(validators[1].severity, Severity::Warning);
        assert!(from_yaml("").unwrap().is_empty());
        assert!(from_yaml("validators: [{name: x, command: []}]").is_err());
        assert!(from_yaml("validators: [{name: x, command: [true], scope: [\"a/*\"]}]").is_err());
    }
}
```

`crates/trellis-validate/src/runner.rs`:

```rust
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use trellis_coord::{CoordError, EntryKind, NewEntry, Scope};
use trellis_core::{Changeset, FileChange, Hash};
use trellis_land::{Engine, LandError};

use crate::spec::{Severity, ValidatorSpec};

/// How much of a failing command's output is kept, from the end.
const OUTPUT_TAIL: usize = 4000;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Land(#[from] LandError),
    #[error(transparent)]
    Coord(#[from] CoordError),
    #[error("io: {0}")]
    Io(#[from] io::Error),
}

/// A failed check on a landed changeset.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub seq: u64,
    pub changeset: Hash,
    pub validator: String,
    pub severity: Severity,
    /// The exit code; `None` if the command timed out, was killed, or could not start.
    pub exit: Option<i32>,
    /// The end of the command's combined output.
    pub output: String,
    /// The log entry recording the finding.
    pub log_seq: u64,
    /// The repair task, for errors.
    pub task: Option<String>,
}

/// Validates one repository's landings in a working directory it owns. The directory
/// tracks `main` incrementally; `<dir>.seq` records the last validated sequence number.
pub struct Runner {
    engine: Arc<Engine>,
    specs: Vec<ValidatorSpec>,
    worktree: PathBuf,
}

fn tail(text: &str, limit: usize) -> String {
    let start = text.len().saturating_sub(limit);
    let start = (start..=text.len()).find(|i| text.is_char_boundary(*i)).unwrap_or(text.len());
    text[start..].to_owned()
}

impl Runner {
    pub fn new(engine: Arc<Engine>, specs: Vec<ValidatorSpec>, worktree: impl Into<PathBuf>) -> Self {
        Self { engine, specs, worktree: worktree.into() }
    }

    fn position_file(&self) -> PathBuf {
        self.worktree.with_extension("seq")
    }

    fn write_file(&self, path: &str, content: &[u8]) -> io::Result<()> {
        let target = self.worktree.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, content)
    }

    /// The last validated sequence number. On first use, materializes the current `main`
    /// and starts from there: history is not re-validated.
    fn position(&self) -> Result<u64, RunError> {
        if let Ok(text) = fs::read_to_string(self.position_file()) {
            if let Ok(seq) = text.trim().parse() {
                return Ok(seq);
            }
        }
        if self.worktree.exists() {
            fs::remove_dir_all(&self.worktree)?;
        }
        fs::create_dir_all(&self.worktree)?;
        let main = self.engine.main()?;
        for file in self.engine.files_at(&main)? {
            self.write_file(file.path.as_str(), &self.engine.read_blob(&file.blob.hash)?)?;
        }
        let seq = self.engine.changesets_since(0)?.last().map_or(0, |(seq, _, _)| *seq);
        fs::write(self.position_file(), seq.to_string())?;
        Ok(seq)
    }

    fn apply(&self, changeset: &Changeset) -> Result<(), RunError> {
        for (path, change) in &changeset.changes {
            match change {
                FileChange::Upsert { blob, .. } => {
                    self.write_file(path.as_str(), &self.engine.read_blob(&blob.hash)?)?;
                }
                FileChange::Delete => match fs::remove_file(self.worktree.join(path.as_str())) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                },
            }
        }
        Ok(())
    }

    /// Validates every changeset landed since the last run, in order.
    pub fn run_pending(&self) -> Result<Vec<Finding>, RunError> {
        if self.specs.is_empty() {
            return Ok(Vec::new());
        }
        let position = self.position()?;
        let mut findings = Vec::new();
        for (seq, id, changeset) in self.engine.changesets_since(position)? {
            self.apply(&changeset)?;
            for spec in &self.specs {
                let relevant =
                    changeset.changes.keys().any(|path| spec.scope.iter().any(|scope| scope.covers(path)));
                if !relevant {
                    continue;
                }
                let (exit, output) = self.execute(spec, seq, &id, &changeset)?;
                if exit != Some(0) {
                    findings.push(self.report(spec, seq, id, &changeset, exit, output)?);
                }
            }
            fs::write(self.position_file(), seq.to_string())?;
        }
        Ok(findings)
    }

    fn execute(&self, spec: &ValidatorSpec, seq: u64, id: &Hash, changeset: &Changeset) -> Result<(Option<i32>, String), RunError> {
        let log_path = self.worktree.with_extension("log");
        let stdout = fs::File::create(&log_path)?;
        let stderr = stdout.try_clone()?;
        let changed: Vec<&str> = changeset.changes.keys().map(|p| p.as_str()).collect();
        let Some((program, args)) = spec.command.split_first() else {
            return Ok((None, format!("validator {} has an empty command", spec.name)));
        };
        let started = Command::new(program)
            .args(args)
            .current_dir(&self.worktree)
            .env("TRELLIS_CHANGESET", id.to_hex())
            .env("TRELLIS_SEQ", seq.to_string())
            .env("TRELLIS_PRINCIPAL", &changeset.provenance.principal)
            .env("TRELLIS_CHANGED", changed.join("\n"))
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn();
        let mut child = match started {
            Ok(child) => child,
            Err(e) => return Ok((None, format!("could not start {:?}: {e}", spec.command))),
        };
        let deadline = Instant::now() + Duration::from_secs(spec.timeout_seconds);
        let mut timed_out = false;
        let exit = loop {
            if let Some(status) = child.try_wait()? {
                break status.code();
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let output = fs::read(&log_path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let mut output = tail(&output, OUTPUT_TAIL);
        if timed_out {
            output.push_str(&format!("\n(timed out after {}s)", spec.timeout_seconds));
        }
        Ok((exit, output))
    }

    fn report(
        &self,
        spec: &ValidatorSpec,
        seq: u64,
        id: Hash,
        changeset: &Changeset,
        exit: Option<i32>,
        output: String,
    ) -> Result<Finding, RunError> {
        let author = format!("validator:{}", spec.name);
        let status = match exit {
            Some(code) => format!("exit {code}"),
            None => "no exit code".to_owned(),
        };
        let body = format!(
            "{} failed on seq {seq} (\"{}\" by {}, {status}).\n\n{output}",
            spec.name, changeset.message, changeset.provenance.principal
        );
        let scope: Vec<Scope> = changeset.changes.keys().cloned().map(Scope::Path).collect();
        let coord = self.engine.coord();
        let kind = match spec.severity {
            Severity::Error => EntryKind::Fail,
            Severity::Warning => EntryKind::Note,
        };
        let entry = coord.append(
            &author,
            NewEntry {
                kind: Some(kind),
                topics: vec!["validator".into(), spec.name.clone()],
                scope: scope.clone(),
                body: body.clone(),
                evidence: vec![id.to_hex()],
            },
        )?;
        let task = match spec.severity {
            Severity::Error => {
                Some(coord.create_task(&author, &format!("Fix {} failure from seq {seq}", spec.name), &body, scope)?.id)
            }
            Severity::Warning => None,
        };
        Ok(Finding { seq, changeset: id, validator: spec.name.clone(), severity: spec.severity, exit, output, log_seq: entry.seq, task })
    }
}
```

- [ ] **Step 2: Write the runner tests**

`crates/trellis-validate/tests/runner.rs`:

```rust
//! The runner against a real engine and real commands.

use std::path::Path;
use std::sync::Arc;

use trellis_coord::{EntryKind, EntryStatus, Scope};
use trellis_core::RepoPath;
use trellis_land::{Engine, Outcome, Policy, TestEnv};
use trellis_store::{MemBlobStore, SqliteMetaStore};
use trellis_validate::{Runner, Severity, ValidatorSpec};

fn engine() -> Arc<Engine> {
    let e = Engine::new(
        "r",
        Arc::new(SqliteMetaStore::open_in_memory().unwrap()),
        Arc::new(MemBlobStore::new()),
        Arc::new(TestEnv::new()),
        Policy::default(),
    );
    e.init().unwrap();
    Arc::new(e)
}

fn land(e: &Engine, principal: &str, edits: &[(&str, &str)]) -> u64 {
    let s = e.open_session(principal, None).unwrap();
    for (path, content) in edits {
        e.write(&s.id, &RepoPath::parse(path).unwrap(), Some(content.as_bytes())).unwrap();
    }
    match e.land(&s.id, &format!("{principal} edits"), "k").unwrap().outcome {
        Outcome::Landed { seq, .. } => seq,
        other => panic!("expected a landing, got {other:?}"),
    }
}

fn spec(name: &str, scope: &[&str], script: &str) -> ValidatorSpec {
    ValidatorSpec {
        name: name.into(),
        scope: scope.iter().map(|s| Scope::parse(s).unwrap()).collect(),
        command: vec!["sh".into(), "-c".into(), script.into()],
        timeout_seconds: 10,
        severity: Severity::Error,
    }
}

fn worktree(tmp: &tempfile::TempDir) -> std::path::PathBuf {
    tmp.path().join("worktrees").join("r")
}

#[test]
fn failures_become_fail_entries_and_repair_tasks() {
    let tmp = tempfile::tempdir().unwrap();
    let e = engine();
    land(&e, "seed", &[("README.md", "# r\n")]);
    let runner = Runner::new(Arc::clone(&e), vec![spec("needs-license", &["**"], "test -f LICENSE")], worktree(&tmp));
    assert!(runner.run_pending().unwrap().is_empty(), "history before the first run is not validated");

    let seq = land(&e, "alice", &[("src/a.rs", "fn a() {}\n")]);
    let findings = runner.run_pending().unwrap();
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!((finding.seq, finding.exit, finding.validator.as_str()), (seq, Some(1), "needs-license"));
    let entry = &e.coord().entries(finding.log_seq - 1, 1).unwrap()[0];
    assert_eq!((entry.kind, entry.status), (EntryKind::Fail, EntryStatus::Open));
    assert_eq!(entry.author, "validator:needs-license");
    assert_eq!(entry.topics, vec!["validator".to_owned(), "needs-license".to_owned()]);
    assert_eq!(entry.evidence, vec![finding.changeset.to_hex()]);
    assert!(entry.body.contains("alice"), "{}", entry.body);
    let task = e.coord().task(finding.task.as_deref().unwrap()).unwrap();
    assert_eq!(task.subject, format!("Fix needs-license failure from seq {seq}"));
    assert_eq!(task.scope, vec![Scope::parse("src/a.rs").unwrap()]);

    land(&e, "bob", &[("LICENSE", "MIT\n")]);
    assert!(runner.run_pending().unwrap().is_empty());
    for path in ["README.md", "src/a.rs", "LICENSE"] {
        assert!(worktree(&tmp).join(path).exists(), "{path} missing from the worktree");
    }
}

#[test]
fn validators_run_only_for_their_scope_and_see_the_landed_tree_and_environment() {
    let tmp = tempfile::tempdir().unwrap();
    let e = engine();
    let checks = vec![
        spec("src-ok", &["src/**"], "grep -qx ok src/a.rs"),
        spec("env", &["src/**"], r#"test "$TRELLIS_PRINCIPAL" = carol && test "$TRELLIS_CHANGED" = src/a.rs && test -n "$TRELLIS_CHANGESET""#),
    ];
    let runner = Runner::new(Arc::clone(&e), checks, worktree(&tmp));
    runner.run_pending().unwrap();
    land(&e, "dave", &[("docs/x.md", "anything\n")]);
    assert!(runner.run_pending().unwrap().is_empty(), "docs changes do not trigger src validators");
    land(&e, "carol", &[("src/a.rs", "bad\n")]);
    let names: Vec<String> = runner.run_pending().unwrap().into_iter().map(|f| f.validator).collect();
    assert_eq!(names, ["src-ok"]);
    land(&e, "carol", &[("src/a.rs", "ok\n")]);
    assert!(runner.run_pending().unwrap().is_empty());
}

#[test]
fn timeouts_and_warnings_are_findings_without_tasks() {
    let tmp = tempfile::tempdir().unwrap();
    let e = engine();
    let mut slow = spec("slow", &["**"], "sleep 5");
    slow.timeout_seconds = 1;
    slow.severity = Severity::Warning;
    let runner = Runner::new(Arc::clone(&e), vec![slow], worktree(&tmp));
    runner.run_pending().unwrap();
    land(&e, "a", &[("x.md", "x\n")]);
    let findings = runner.run_pending().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].exit, None);
    assert!(findings[0].output.contains("timed out after 1s"));
    assert_eq!(findings[0].task, None);
    assert_eq!(e.coord().entries(0, 10).unwrap()[0].kind, EntryKind::Note);
    assert!(e.coord().tasks().unwrap().is_empty());
}

#[test]
fn a_new_runner_resumes_from_the_recorded_position() {
    let tmp = tempfile::tempdir().unwrap();
    let e = engine();
    let checks = || vec![spec("never", &["**"], "false")];
    Runner::new(Arc::clone(&e), checks(), worktree(&tmp)).run_pending().unwrap();
    land(&e, "a", &[("one.md", "1\n")]);
    assert_eq!(Runner::new(Arc::clone(&e), checks(), worktree(&tmp)).run_pending().unwrap().len(), 1);
    land(&e, "a", &[("two.md", "2\n")]);
    let again = Runner::new(Arc::clone(&e), checks(), worktree(&tmp)).run_pending().unwrap();
    assert_eq!(again.len(), 1, "only the new landing is validated");
    assert!(Path::new(&worktree(&tmp)).join("one.md").exists());
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p trellis-validate`
Expected: unit `test result: ok. 1 passed`; `tests/runner.rs` `test result: ok. 4 passed`.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/trellis-validate
git commit -m "feat(validate): run checks on landed changesets; failures become FAIL entries and tasks"
```

---

### Task 3: Run validators in the server

**Files:**
- Modify: `crates/trellis-server/Cargo.toml`, `src/lib.rs`, `src/state.rs`, `tests/api.rs`; `crates/trellis-cli/Cargo.toml`, `src/main.rs`

- [ ] **Step 1: Configuration and the background loop**

In `crates/trellis-server/Cargo.toml` `[dependencies]`, add `trellis-validate = { workspace = true }`.

In `crates/trellis-server/src/state.rs`:

- add `use trellis_validate::{Runner, ValidatorSpec};`, `use std::sync::atomic::{AtomicBool, Ordering};`, and `use std::time::Duration;`
- add a field `pub validators: Vec<ValidatorSpec>,` to `Config`, initialized to `Vec::new()` in `Config::new`
- add fields `validators: Vec<ValidatorSpec>,` and `data_dir: PathBuf,` to `AppState`, filled from the config in `AppState::open`
- add to `impl AppState`:

```rust
    /// Runs the configured validators for every active repository, about twice a second,
    /// until `stop` is set. Does nothing without validators.
    pub fn spawn_validators(self: &Arc<Self>, stop: Arc<AtomicBool>) {
        if self.validators.is_empty() {
            return;
        }
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let mut runners: HashMap<String, Runner> = HashMap::new();
            while !stop.load(Ordering::Relaxed) {
                let engines: Vec<(String, Arc<Engine>)> = state
                    .engines
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .iter()
                    .map(|(repo, engine)| (repo.clone(), Arc::clone(engine)))
                    .collect();
                for (repo, engine) in engines {
                    if engine.main().is_err() {
                        continue;
                    }
                    let runner = runners.entry(repo.clone()).or_insert_with(|| {
                        Runner::new(engine, state.validators.clone(), state.data_dir.join("worktrees").join(&repo))
                    });
                    if let Err(e) = runner.run_pending() {
                        eprintln!("trellis: validators for {repo}: {e}");
                    }
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }
```

In `crates/trellis-server/src/lib.rs`:

- in `run`, after creating `state`, call `state.spawn_validators(Arc::new(std::sync::atomic::AtomicBool::new(false)));`
- in `start_background`, create `let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));`, call `state.spawn_validators(Arc::clone(&stop));`, and store `stop` in `Background` (new field `stop: Arc<std::sync::atomic::AtomicBool>`); in `Drop for Background`, also call `self.stop.store(true, std::sync::atomic::Ordering::Relaxed);`.

- [ ] **Step 2: Write the server test**

Append to `crates/trellis-server/tests/api.rs`:

```rust
#[test]
fn validators_run_after_landings_in_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new("127.0.0.1:0".parse().unwrap(), dir.path().join("data"));
    config.validators = trellis_validate::from_yaml(
        "validators:\n  - name: needs-license\n    command: [sh, -c, test -f LICENSE]\n",
    )
    .unwrap();
    let bg = start_background(config).unwrap();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();
    // Let the runner record its starting position before anything lands.
    std::thread::sleep(std::time::Duration::from_millis(1500));

    let s = c.open_session("alice", None).unwrap();
    c.write(&s.id, "src/a.rs", b"fn a() {}\n").unwrap();
    landed(&c.land(&s.id, "add a", "k").unwrap().outcome);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let entry = loop {
        let fails: Vec<_> = c.log_entries(0, 100).unwrap().into_iter().filter(|e| e.topics.contains(&"validator".to_owned())).collect();
        if let Some(entry) = fails.into_iter().next() {
            break entry;
        }
        assert!(std::time::Instant::now() < deadline, "no validator finding appeared");
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    assert_eq!(entry.author, "validator:needs-license");
    assert!(c.tasks().unwrap().iter().any(|t| t.subject.starts_with("Fix needs-license failure")));
}
```

- [ ] **Step 3: `tl serve --policy` loads validators too**

In `crates/trellis-cli/Cargo.toml` `[dependencies]`, add `trellis-validate = { workspace = true }`.

In `crates/trellis-cli/src/main.rs`, replace the `Command::Serve` arm with:

```rust
        Command::Serve { addr, data, policy } => {
            let mut config = trellis_server::Config::new(addr, data);
            if let Some(path) = policy {
                let text = fs::read_to_string(path)?;
                config.policy = Policy::from_yaml(&text)?;
                config.validators = trellis_validate::from_yaml(&text)?;
            }
            trellis_server::run(config).map_err(|e| e.to_string())?;
        }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p trellis-server && cargo test -p trellis-cli`
Expected: server `tests/api.rs` `test result: ok. 7 passed`; cli tests pass.

- [ ] **Step 5: README and verification**

In `README.md`, add after the Git Bridge section:

````markdown
## Validators

Checks run after every landing instead of gating it (Scion's post-merge testing). Declare them in the policy file passed to `tl serve --policy trellis.policy.yaml`:

```yaml
rules:
  - pattern: "canon/**"
    mode: strict
validators:
  - name: tests
    scope: ["src/**"]          # run when a landing changes these paths (default: everything)
    command: ["cargo", "test", "--quiet"]
    timeout_seconds: 900
    severity: error            # error: FAIL entry + repair task; warning: NOTE only
```

The command runs in a checkout of `main` at the landed changeset, kept up to date incrementally, with `TRELLIS_CHANGESET`, `TRELLIS_SEQ`, `TRELLIS_PRINCIPAL`, and `TRELLIS_CHANGED` set. Failures appear in the log under the `validator` topic and as tasks named `Fix <name> failure from seq N`.
````

Update the status line to mention validators. Then run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`. Expected: clippy exits 0; all suites `ok`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.lock crates/trellis-server crates/trellis-cli README.md docs/superpowers/plans/2026-10-15-m4a-validators.md
git commit -m "feat(server): run validators after landings; tl serve loads them from the policy file"
```

# M3c: Git Bridge — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect Trellis to Git (spec §4.10): import a Git tree into Trellis, and export landed changesets back to Git as commits, so code review, CI, and history keep working.

**Architecture:** A new `trellis-git` crate drives the `git` command line (no Git library). `import` reads a commit's tree with `git ls-tree -r -z` and contents with one long-lived `git cat-file --batch`, writes only files whose content differs from Trellis's view, deletes files Git no longer has, and lands one changeset. `export` replays every changeset after the last exported sequence number into the worktree and commits each one with the changeset's principal as author and `Trellis-*` trailers. Progress lives in `.git/trellis-export.json`. Safety: export refuses a dirty worktree; import refuses when Trellis has changes not yet exported (they would otherwise be skipped), and records its own changeset as exported (Git already has it).

**Tech Stack:** `git` CLI via `std::process`; `trellis-client`.

**Spec:** §4.10 (v1: one-way continuous export; imports are snapshots, not history replays — history stays in Git).

---

### Task 1: The bridge

**Files:**
- Modify: `Cargo.toml` (add `trellis-git = { path = "crates/trellis-git" }`)
- Create: `crates/trellis-git/Cargo.toml`, `crates/trellis-git/src/lib.rs`, `crates/trellis-git/tests/bridge.rs`

- [ ] **Step 1: Create the crate**

`crates/trellis-git/Cargo.toml`:

```toml
[package]
name = "trellis-git"
description = "The Trellis Git bridge: import a Git tree, export landed changesets as Git commits."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-client = { workspace = true }
trellis-core = { workspace = true }
trellis-land = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
trellis-server = { workspace = true }
tempfile = { workspace = true }
```

`crates/trellis-git/src/lib.rs`:

```rust
//! The Git bridge (spec §4.10): import a Git tree into Trellis, and export Trellis's `main`
//! back to Git as one commit per landed changeset. Drives the `git` command line.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde::{Deserialize, Serialize};
use trellis_client::{Client, ClientError};
use trellis_core::{FileChange, Hash, RepoPath};
use trellis_land::{Outcome, Receipt};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("git {args}: {stderr}")]
    Git { args: String, stderr: String },
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, GitError>;

const STATE_FILE: &str = "trellis-export.json";

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    if !out.status.success() {
        return Err(GitError::Git {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    Ok(out.stdout)
}

fn git_text(dir: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&git(dir, args)?).trim().to_owned())
}

/// How far `main` has been exported to this Git repository.
#[derive(Serialize, Deserialize)]
struct ExportState {
    repo: String,
    last_seq: u64,
}

fn state_path(dir: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(git_text(dir, &["rev-parse", "--absolute-git-dir"])?).join(STATE_FILE))
}

fn load_state(dir: &Path, client: &Client) -> Result<ExportState> {
    match fs::read(state_path(dir)?) {
        Ok(bytes) => {
            let state: ExportState = serde_json::from_slice(&bytes)?;
            if state.repo != client.repo() {
                return Err(GitError::Invalid(format!(
                    "this Git repository is bridged to Trellis repository {:?}, not {:?}",
                    state.repo,
                    client.repo()
                )));
            }
            Ok(state)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(ExportState { repo: client.repo().to_owned(), last_seq: 0 })
        }
        Err(e) => Err(e.into()),
    }
}

fn save_state(dir: &Path, state: &ExportState) -> Result<()> {
    fs::write(state_path(dir)?, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

/// Regular files of a commit's tree: (path, blob id). Symlinks and submodules are skipped.
fn tree_files(dir: &Path, commit: &str) -> Result<Vec<(String, String)>> {
    let out = git(dir, &["ls-tree", "-r", "-z", "--full-tree", commit])?;
    let mut files = Vec::new();
    for record in out.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = String::from_utf8_lossy(record);
        let Some((meta, path)) = record.split_once('\t') else { continue };
        let parts: Vec<&str> = meta.split(' ').collect();
        if parts.len() == 3 && parts[1] == "blob" && parts[0] != "120000" {
            files.push((path.to_owned(), parts[2].to_owned()));
        }
    }
    Ok(files)
}

/// One `git cat-file --batch` process, asked for one object at a time.
struct CatFile {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl CatFile {
    fn open(dir: &Path) -> Result<Self> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or_else(|| GitError::Invalid("git cat-file has no stdout".into()))?;
        Ok(Self { child, stdin, stdout: BufReader::new(stdout) })
    }

    fn read(&mut self, id: &str) -> Result<Vec<u8>> {
        let stdin = self.stdin.as_mut().ok_or_else(|| GitError::Invalid("git cat-file is closed".into()))?;
        writeln!(stdin, "{id}")?;
        stdin.flush()?;
        let mut header = String::new();
        self.stdout.read_line(&mut header)?;
        let size: usize = header
            .trim()
            .rsplit(' ')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| GitError::Invalid(format!("unexpected git cat-file output {header:?}")))?;
        let mut content = vec![0; size];
        self.stdout.read_exact(&mut content)?;
        let mut newline = [0u8; 1];
        self.stdout.read_exact(&mut newline)?;
        Ok(content)
    }
}

impl Drop for CatFile {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.wait();
    }
}

#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub commit: String,
    pub written: usize,
    pub deleted: usize,
    /// Paths Trellis cannot represent (for example with a backslash), left out.
    pub skipped: Vec<String>,
    pub receipt: Receipt,
}

/// Imports the tree of `rev` as one changeset that makes Trellis's `main` match it.
/// Refuses while Trellis has changes not yet exported to this repository.
pub fn import(client: &Client, principal: &str, dir: &Path, rev: &str) -> Result<ImportReport> {
    let mut state = load_state(dir, client)?;
    let pending = client.changesets_since(state.last_seq)?;
    if !pending.is_empty() {
        return Err(GitError::Invalid(format!(
            "Trellis has {} changeset(s) not yet exported to this Git repository; run export first",
            pending.len()
        )));
    }
    let commit = git_text(dir, &["rev-parse", &format!("{rev}^{{commit}}")])?;
    let session = client.open_session(principal, None)?;
    let existing: BTreeMap<String, Hash> =
        client.files(&session.id, &[])?.into_iter().map(|f| (f.path.to_string(), f.blob.hash)).collect();
    let mut cat = CatFile::open(dir)?;
    let mut seen = BTreeSet::new();
    let mut written = 0;
    let mut skipped = Vec::new();
    for (path, id) in tree_files(dir, &commit)? {
        if RepoPath::parse(&path).is_err() {
            skipped.push(path);
            continue;
        }
        let content = cat.read(&id)?;
        if existing.get(&path) != Some(&Hash::of(&content)) {
            client.write(&session.id, &path, &content)?;
            written += 1;
        }
        seen.insert(path);
    }
    let mut deleted = 0;
    for path in existing.keys() {
        if !seen.contains(path) {
            client.delete(&session.id, path)?;
            deleted += 1;
        }
    }
    let short = &commit[..commit.len().min(12)];
    let receipt = client.land(&session.id, &format!("Import {short} from Git"), &format!("git-import-{commit}"))?;
    if let Outcome::Landed { seq, .. } = &receipt.outcome {
        // Git already has this changeset's content.
        state.last_seq = *seq;
        save_state(dir, &state)?;
    }
    Ok(ImportReport { commit, written, deleted, skipped, receipt })
}

#[derive(Debug, Serialize)]
pub struct ExportedCommit {
    pub seq: u64,
    pub changeset: Hash,
    pub commit: String,
}

#[derive(Debug, Serialize)]
pub struct ExportReport {
    pub commits: Vec<ExportedCommit>,
}

fn email_local_part(principal: &str) -> String {
    principal
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_') { c } else { '-' })
        .collect()
}

/// Commits every changeset of `main` not yet exported, in order, one Git commit each.
/// Refuses a worktree with uncommitted changes.
pub fn export(client: &Client, dir: &Path) -> Result<ExportReport> {
    if !git(dir, &["status", "--porcelain"])?.is_empty() {
        return Err(GitError::Invalid(
            "the Git worktree has uncommitted changes; commit or stash them first".into(),
        ));
    }
    let mut state = load_state(dir, client)?;
    let mut commits = Vec::new();
    for entry in client.changesets_since(state.last_seq)? {
        let changeset = &entry.changeset;
        for (path, change) in &changeset.changes {
            let target = dir.join(path.as_str());
            match change {
                FileChange::Upsert { blob, .. } => {
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&target, client.blob(&blob.hash)?)?;
                }
                FileChange::Delete => match fs::remove_file(&target) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                },
            }
        }
        git(dir, &["add", "-A"])?;
        let p = &changeset.provenance;
        let subject = if changeset.message.is_empty() { "(no message)" } else { changeset.message.as_str() };
        let mut message = format!(
            "{subject}\n\nTrellis-Changeset: {}\nTrellis-Seq: {}\nTrellis-Principal: {}",
            entry.id, entry.seq, p.principal
        );
        if let Some(session) = &p.session {
            message.push_str(&format!("\nTrellis-Session: {session}"));
        }
        if let Some(task) = &p.task {
            message.push_str(&format!("\nTrellis-Task: {task}"));
        }
        let author = format!("{} <{}@trellis.invalid>", p.principal, email_local_part(&p.principal));
        git(
            dir,
            &[
                "-c",
                "user.name=Trellis",
                "-c",
                "user.email=trellis@trellis.invalid",
                "commit",
                "--quiet",
                "--no-verify",
                "--allow-empty",
                "--author",
                &author,
                "-m",
                &message,
            ],
        )?;
        let commit = git_text(dir, &["rev-parse", "HEAD"])?;
        state.last_seq = entry.seq;
        save_state(dir, &state)?;
        commits.push(ExportedCommit { seq: entry.seq, changeset: entry.id, commit });
    }
    Ok(ExportReport { commits })
}
```

- [ ] **Step 2: Write the tests**

`crates/trellis-git/tests/bridge.rs`:

```rust
//! Import from and export to a real Git repository through a real Trellis server.

use std::path::Path;
use std::process::Command;

use trellis_client::Client;
use trellis_git::{GitError, export, import};
use trellis_land::Outcome;
use trellis_server::{Background, Config, start_background};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Human", "-c", "user.email=human@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn setup() -> (Background, Client, tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let bg = start_background(Config::new("127.0.0.1:0".parse().unwrap(), tmp.path().join("data"))).unwrap();
    let client = Client::new(&bg.url(), "demo").unwrap();
    client.init().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("README.md"), "# demo\n").unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn one() -> u8 { 1 }\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "initial"]);
    (bg, client, tmp, repo)
}

fn land(client: &Client, principal: &str, edits: &[(&str, Option<&str>)], message: &str) {
    let s = client.open_session(principal, Some("T-7")).unwrap();
    for (path, content) in edits {
        match content {
            Some(text) => client.write(&s.id, path, text.as_bytes()).unwrap(),
            None => client.delete(&s.id, path).unwrap(),
        }
    }
    assert!(matches!(client.land(&s.id, message, "k").unwrap().outcome, Outcome::Landed { .. }));
}

#[test]
fn import_then_export_round_trips_through_git() {
    let (_bg, c, _tmp, repo) = setup();
    let report = import(&c, "importer", &repo, "HEAD").unwrap();
    assert_eq!((report.written, report.deleted), (2, 0));
    assert_eq!(c.read_main("src/lib.rs").unwrap().as_deref(), Some(&b"pub fn one() -> u8 { 1 }\n"[..]));
    // The import itself is not exported again.
    assert!(export(&c, &repo).unwrap().commits.is_empty());

    land(&c, "alice", &[("src/lib.rs", Some("pub fn one() -> u8 { 2 }\n")), ("README.md", None), ("docs/new.md", Some("new\n"))], "alice's change");
    let exported = export(&c, &repo).unwrap();
    assert_eq!(exported.commits.len(), 1);
    assert_eq!(std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(), "pub fn one() -> u8 { 2 }\n");
    assert!(!repo.join("README.md").exists());
    assert_eq!(std::fs::read_to_string(repo.join("docs/new.md")).unwrap(), "new\n");
    assert_eq!(git(&repo, &["log", "-1", "--format=%an <%ae>"]), "alice <alice@trellis.invalid>");
    let body = git(&repo, &["log", "-1", "--format=%B"]);
    assert!(body.starts_with("alice's change"), "{body}");
    assert!(body.contains(&format!("Trellis-Changeset: {}", exported.commits[0].changeset)), "{body}");
    assert!(body.contains("Trellis-Task: T-7"), "{body}");
    assert!(git(&repo, &["status", "--porcelain"]).is_empty());
    assert!(export(&c, &repo).unwrap().commits.is_empty());

    // A human commits in Git; importing brings it into Trellis without re-exporting it.
    std::fs::write(repo.join("docs/new.md"), "edited by a human\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "human edit"]);
    let second = import(&c, "importer", &repo, "HEAD").unwrap();
    assert_eq!((second.written, second.deleted), (1, 0));
    assert_eq!(c.read_main("docs/new.md").unwrap().as_deref(), Some(&b"edited by a human\n"[..]));
    assert!(export(&c, &repo).unwrap().commits.is_empty());
}

#[test]
fn import_refuses_to_skip_unexported_changes() {
    let (_bg, c, _tmp, repo) = setup();
    import(&c, "importer", &repo, "HEAD").unwrap();
    land(&c, "alice", &[("x.md", Some("x\n"))], "x");
    let err = import(&c, "importer", &repo, "HEAD").unwrap_err();
    assert!(matches!(err, GitError::Invalid(ref m) if m.contains("export first")), "{err}");
    assert_eq!(export(&c, &repo).unwrap().commits.len(), 1);
    import(&c, "importer", &repo, "HEAD").unwrap();
}

#[test]
fn export_refuses_a_dirty_worktree() {
    let (_bg, c, _tmp, repo) = setup();
    import(&c, "importer", &repo, "HEAD").unwrap();
    land(&c, "alice", &[("y.md", Some("y\n"))], "y");
    std::fs::write(repo.join("scratch.txt"), "uncommitted").unwrap();
    let err = export(&c, &repo).unwrap_err();
    assert!(matches!(err, GitError::Invalid(ref m) if m.contains("uncommitted")), "{err}");
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p trellis-git`
Expected: `test result: ok. 3 passed`.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/trellis-git
git commit -m "feat(git): add the Git bridge: import a tree, export changesets as commits"
```

---

### Task 2: `tl git import|export`, docs, verification

- [ ] **Step 1: Add the commands**

In `crates/trellis-cli/Cargo.toml` `[dependencies]`, add `trellis-git = { workspace = true }`.

In `crates/trellis-cli/src/main.rs`, add to `Command` (after `Mcp`):

```rust
    /// Bridge to a Git repository.
    Git {
        #[command(subcommand)]
        action: GitAction,
    },
```

add the subcommand enum:

```rust
#[derive(Subcommand)]
enum GitAction {
    /// Make main match a Git commit's tree (one changeset).
    Import {
        dir: PathBuf,
        #[arg(long, default_value = "HEAD")]
        rev: String,
    },
    /// Commit main's changesets not yet exported, one Git commit each.
    Export { dir: PathBuf },
}
```

and the match arm (after `Command::Mcp`):

```rust
        Command::Git { action } => match action {
            GitAction::Import { dir, rev } => print(&trellis_git::import(&client()?, &cli.principal, &dir, &rev)?)?,
            GitAction::Export { dir } => print(&trellis_git::export(&client()?, &dir)?)?,
        },
```

- [ ] **Step 2: README**

In `README.md`, update the status line to say M3c (the Git bridge) is built, mark the **M3 Interfaces** roadmap bullet with ` ✅`, and add after the MCP section:

````markdown
## Git Bridge

```bash
tl --as importer git import ./my-repo     # main now matches my-repo's HEAD (one changeset)
# ... agents land work through Trellis ...
tl git export ./my-repo                   # one Git commit per landed changeset, authored by its agent,
                                          # with Trellis-Changeset/Seq/Session/Task trailers
```

Export refuses a dirty worktree; import refuses while Trellis has changes not yet exported.
````

- [ ] **Step 3: Verify everything**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: clippy exits 0; all suites `ok`.

- [ ] **Step 4: Commit**

```bash
git add Cargo.lock crates/trellis-cli README.md docs/superpowers/plans/2026-10-15-m3c-git-bridge.md
git commit -m "feat(cli): add tl git import and export; M3 complete"
```

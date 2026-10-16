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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ExportState {
            repo: client.repo().to_owned(),
            last_seq: 0,
        }),
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
        let Some((meta, path)) = record.split_once('\t') else {
            continue;
        };
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
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| GitError::Invalid("git cat-file has no stdout".into()))?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    fn read(&mut self, id: &str) -> Result<Vec<u8>> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| GitError::Invalid("git cat-file is closed".into()))?;
        writeln!(stdin, "{id}")?;
        stdin.flush()?;
        let mut header = String::new();
        self.stdout.read_line(&mut header)?;
        let size: usize = header
            .trim()
            .rsplit(' ')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| {
                GitError::Invalid(format!("unexpected git cat-file output {header:?}"))
            })?;
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
    let existing: BTreeMap<String, Hash> = client
        .files(&session.id, &[])?
        .into_iter()
        .map(|f| (f.path.to_string(), f.blob.hash))
        .collect();
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
    let receipt = client.land(
        &session.id,
        &format!("Import {short} from Git"),
        &format!("git-import-{commit}"),
    )?;
    if let Outcome::Landed { seq, .. } = &receipt.outcome {
        // Git already has this changeset's content.
        state.last_seq = *seq;
        save_state(dir, &state)?;
    }
    Ok(ImportReport {
        commit,
        written,
        deleted,
        skipped,
        receipt,
    })
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
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_') {
                c
            } else {
                '-'
            }
        })
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
        let subject = if changeset.message.is_empty() {
            "(no message)"
        } else {
            changeset.message.as_str()
        };
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
        let author = format!(
            "{} <{}@trellis.invalid>",
            p.principal,
            email_local_part(&p.principal)
        );
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
        commits.push(ExportedCommit {
            seq: entry.seq,
            changeset: entry.id,
            commit,
        });
    }
    Ok(ExportReport { commits })
}

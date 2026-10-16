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
    let start = (start..=text.len())
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(text.len());
    text[start..].to_owned()
}

impl Runner {
    pub fn new(
        engine: Arc<Engine>,
        specs: Vec<ValidatorSpec>,
        worktree: impl Into<PathBuf>,
    ) -> Self {
        Self {
            engine,
            specs,
            worktree: worktree.into(),
        }
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
        let seq = self
            .engine
            .changesets_since(0)?
            .last()
            .map_or(0, |(seq, _, _)| *seq);
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
                let relevant = changeset
                    .changes
                    .keys()
                    .any(|path| spec.scope.iter().any(|scope| scope.covers(path)));
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

    fn execute(
        &self,
        spec: &ValidatorSpec,
        seq: u64,
        id: &Hash,
        changeset: &Changeset,
    ) -> Result<(Option<i32>, String), RunError> {
        let log_path = self.worktree.with_extension("log");
        let stdout = fs::File::create(&log_path)?;
        let stderr = stdout.try_clone()?;
        let changed: Vec<&str> = changeset.changes.keys().map(|p| p.as_str()).collect();
        let Some((program, args)) = spec.command.split_first() else {
            return Ok((
                None,
                format!("validator {} has an empty command", spec.name),
            ));
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
        let output = fs::read(&log_path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
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
            Severity::Error => Some(
                coord
                    .create_task(
                        &author,
                        &format!("Fix {} failure from seq {seq}", spec.name),
                        &body,
                        scope,
                    )?
                    .id,
            ),
            Severity::Warning => None,
        };
        Ok(Finding {
            seq,
            changeset: id,
            validator: spec.name.clone(),
            severity: spec.severity,
            exit,
            output,
            log_seq: entry.seq,
            task,
        })
    }
}

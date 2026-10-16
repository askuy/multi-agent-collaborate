use rand::Rng;
use rand_chacha::ChaCha8Rng;
use trellis_coord::{ClaimOutcome, EntryKind, NewEntry, Scope, TaskStatus};
use trellis_core::RepoPath;
use trellis_land::{Engine, Intent, Outcome, RebaseOutcome, Receipt, Resolutions};

/// Files the agents fight over: prose (line merges), a JSON canon (field merges), a strict
/// file (never merged), and a serialized file.
pub const UNIVERSE: [&str; 6] = [
    "notes/a.md",
    "notes/b.md",
    "canon/world.json",
    "src/lib.rs",
    "strict/rules.md",
    "serial/Cargo.lock",
];
const SCOPES: [&str; 5] = [
    "notes/**",
    "notes/a.md",
    "canon/**",
    "src/lib.rs",
    "serial/**",
];
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
    RepoPath::parse(UNIVERSE[rng.random_range(0..UNIVERSE.len())])
        .expect("universe paths are valid")
}

fn scope(rng: &mut ChaCha8Rng) -> Scope {
    Scope::parse(SCOPES[rng.random_range(0..SCOPES.len())]).expect("scopes are valid")
}

/// New content for `path` derived from what the session currently sees.
fn edit(
    engine: &Engine,
    session: &str,
    path: &RepoPath,
    agent: &str,
    rng: &mut ChaCha8Rng,
) -> Option<Vec<u8>> {
    let current = engine.read(session, path).ok()?;
    if path.as_str().ends_with(".json") {
        let mut doc: serde_json::Map<String, serde_json::Value> = current
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        doc.insert(
            format!("k{}", rng.random_range(0..8)),
            serde_json::json!(rng.random_range(0..1000)),
        );
        return serde_json::to_vec_pretty(&doc).ok();
    }
    let mut lines: Vec<String> = match current {
        Some(bytes) => String::from_utf8_lossy(&bytes)
            .lines()
            .map(str::to_owned)
            .collect(),
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
                Ok(RebaseOutcome::AlreadyLanded { .. }) => {
                    self.finish(engine);
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
                    Ok(receipt) => Event::Receipt {
                        receipt,
                        first_try: false,
                    },
                    Err(_) => Event::Nothing,
                },
                None => Event::Nothing,
            },
            84..89 => {
                let kind = if rng.random_bool(0.3) {
                    EntryKind::Fail
                } else {
                    EntryKind::Fact
                };
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
                let topics: Vec<String> = if rng.random_bool(0.5) {
                    Vec::new()
                } else {
                    vec![TOPICS[rng.random_range(0..TOPICS.len())].to_owned()]
                };
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
                let watch = Intent {
                    description: "watch".into(),
                    reads: vec![scope(rng)],
                    ..Default::default()
                };
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
                let open: Vec<_> = tasks
                    .into_iter()
                    .filter(|t| t.status == TaskStatus::Open)
                    .collect();
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
                    Outcome::Landed { .. } | Outcome::Unchanged { .. } => self.finish(engine),
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
            Ok(RebaseOutcome::AlreadyLanded { .. }) => {
                self.conflict = None;
                self.finish(engine);
            }
            Err(_) => self.conflict = None,
        }
        Event::Nothing
    }

    /// The session is done (landed or unchanged): drop it and complete its task.
    fn finish(&mut self, engine: &Engine) {
        self.session = None;
        if let Some(task) = self.task.take() {
            let _ = engine.coord().complete_task(&task, &self.name, "landed");
        }
    }
}

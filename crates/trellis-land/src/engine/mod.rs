use std::collections::BTreeMap;
use std::sync::Arc;

use trellis_coord::{Coord, Env, Guard, KeyedLocks, Scope};
use trellis_core::{Changeset, FileChange, FileKind, Hash, Provenance, RepoPath};
use trellis_derive::{DeriveCtx, DeriveError, SkipList, TreeIndex, flatten, read_file};
use trellis_store::{BlobStore, MetaStore};

use crate::error::{LandError, Result};
use crate::policy::Policy;
use crate::records::{FileEntry, SESSION, SessionRecord, SessionStatus, decode, encode};

mod conflict;
mod intent;
mod land;
mod merge;
mod notify;

pub use conflict::RebaseOutcome;
pub use intent::{Intent, IntentOutcome};

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
    coord: Coord,
}

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
        let repo = repo.into();
        let coord = Coord::new(repo.clone(), Arc::clone(&meta), Arc::clone(&env));
        Self {
            repo,
            meta,
            blobs,
            env,
            policy,
            locks: KeyedLocks::default(),
            coord,
        }
    }

    /// How many unfinished tasks one principal may hold at once (default 1).
    pub fn with_max_claims(mut self, max_claims: usize) -> Self {
        self.coord = self.coord.with_max_claims(max_claims);
        self
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
            provenance: Provenance {
                principal: "trellis".into(),
                ..Default::default()
            },
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
            reads: Default::default(),
            watch: Vec::new(),
            leases: Vec::new(),
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
        Ok(self
            .meta
            .put_record(&self.repo, SESSION, &session.id, &encode(session)?)?)
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
    ///
    /// Reads from the base are recorded, so a later landing that changes the path notifies
    /// this session.
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

    pub fn read_main(&self, path: &RepoPath) -> Result<Option<Vec<u8>>> {
        Ok(read_file(&self.ctx(), &self.main()?, path)?)
    }

    pub fn read_blob(&self, hash: &Hash) -> Result<Vec<u8>> {
        self.blobs
            .get(hash)?
            .ok_or(LandError::Derive(DeriveError::MissingObject(*hash)))
    }

    /// The files `session` sees (its base plus its own writes), limited to `scopes`
    /// (empty means everything), in path order.
    pub fn session_files(&self, session: &str, scopes: &[Scope]) -> Result<Vec<FileEntry>> {
        let record = self.session(session)?;
        let ctx = self.ctx();
        let root = ctx.get::<TreeIndex>(&record.base)?.root;
        let mut files: BTreeMap<RepoPath, FileEntry> = flatten(&ctx, &root)?
            .into_iter()
            .map(|(path, (blob, kind))| {
                (
                    path.clone(),
                    FileEntry {
                        path,
                        blob,
                        kind,
                        changed: false,
                    },
                )
            })
            .collect();
        for (path, change) in &record.overlay {
            match change {
                FileChange::Upsert { blob, kind } => {
                    files.insert(
                        path.clone(),
                        FileEntry {
                            path: path.clone(),
                            blob: *blob,
                            kind: *kind,
                            changed: true,
                        },
                    );
                }
                FileChange::Delete => {
                    files.remove(path);
                }
            }
        }
        Ok(files
            .into_values()
            .filter(|f| scopes.is_empty() || scopes.iter().any(|s| s.covers(&f.path)))
            .collect())
    }

    /// Every file of the tree at `changeset`, in path order.
    pub fn files_at(&self, changeset: &Hash) -> Result<Vec<FileEntry>> {
        let ctx = self.ctx();
        let root = ctx.get::<TreeIndex>(changeset)?.root;
        Ok(flatten(&ctx, &root)?
            .into_iter()
            .map(|(path, (blob, kind))| FileEntry {
                path,
                blob,
                kind,
                changed: false,
            })
            .collect())
    }

    /// `main`'s changesets with a sequence number above `after_seq`, oldest first.
    pub fn changesets_since(&self, after_seq: u64) -> Result<Vec<(u64, Hash, Changeset)>> {
        let ctx = self.ctx();
        let mut out = Vec::new();
        let mut current = self.main()?;
        loop {
            let seq = ctx.get::<SkipList>(&current)?.generation;
            if seq <= after_seq {
                break;
            }
            let changeset = ctx.changeset(&current)?;
            let parent = changeset.first_parent();
            out.push((seq, current, changeset));
            match parent {
                Some(parent) => current = parent,
                None => break,
            }
        }
        out.reverse();
        Ok(out)
    }

    /// Abandons an open session; its writes are discarded.
    pub fn close_session(&self, session: &str) -> Result<()> {
        let _guard = self.lock_session(session);
        let mut record = self.open_session_record(session)?;
        record.status = SessionStatus::Closed;
        record.overlay.clear();
        self.save_session(&record)?;
        self.release_session_leases(&record)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use trellis_store::{MemBlobStore, SqliteMetaStore};

    use super::*;
    use trellis_coord::TestEnv;

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

    pub(crate) fn seed(e: &Engine, path: &str, content: &[u8]) {
        let s = e.open_session("seed", None).unwrap();
        e.write(&s.id, &p(path), Some(content)).unwrap();
        assert!(matches!(
            e.land(&s.id, "seed", "k").unwrap().outcome,
            crate::Outcome::Landed { .. }
        ));
    }

    #[test]
    fn session_files_overlay_writes_on_the_base_within_scopes() {
        let e = engine();
        seed(&e, "a/x.md", b"1");
        let s = e.open_session("s", None).unwrap();
        e.write(&s.id, &p("a/y.md"), Some(b"2")).unwrap();
        e.write(&s.id, &p("a/x.md"), None).unwrap();
        e.write(&s.id, &p("b/z.md"), Some(b"3")).unwrap();
        let all = e.session_files(&s.id, &[]).unwrap();
        assert_eq!(
            all.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["a/y.md", "b/z.md"]
        );
        assert!(all.iter().all(|f| f.changed));
        let only_a = e
            .session_files(&s.id, &[trellis_coord::Scope::parse("a/**").unwrap()])
            .unwrap();
        assert_eq!(
            only_a.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["a/y.md"]
        );
        let fresh = e.open_session("t", None).unwrap();
        let base = e.session_files(&fresh.id, &[]).unwrap();
        assert_eq!(base.len(), 1);
        assert!(!base[0].changed);
    }

    #[test]
    fn changesets_since_lists_mains_history_in_order() {
        let e = engine();
        seed(&e, "a", b"1");
        seed(&e, "b", b"2");
        seed(&e, "c", b"3");
        let all = e.changesets_since(0).unwrap();
        assert_eq!(
            all.iter().map(|(seq, _, _)| *seq).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(all[2].1, e.main().unwrap());
        assert_eq!(e.changesets_since(2).unwrap().len(), 1);
        assert!(e.changesets_since(3).unwrap().is_empty());
    }

    #[test]
    fn files_at_lists_mains_tree_at_a_changeset() {
        let e = engine();
        let first = {
            seed(&e, "a.md", b"1");
            e.main().unwrap()
        };
        seed(&e, "b.md", b"2");
        let paths = |id: &Hash| -> Vec<String> {
            e.files_at(id)
                .unwrap()
                .iter()
                .map(|f| f.path.to_string())
                .collect()
        };
        assert_eq!(paths(&first), ["a.md"]);
        assert_eq!(paths(&e.main().unwrap()), ["a.md", "b.md"]);
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
        assert_eq!(
            text(e.read(&s.id, &p("a.txt")).unwrap()).as_deref(),
            Some("hello")
        );
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
        assert!(matches!(
            e.write(&s.id, &p("a"), Some(b"x")),
            Err(LandError::SessionNotOpen(_))
        ));
        assert!(matches!(
            e.session("nope"),
            Err(LandError::SessionNotFound(_))
        ));
    }
}

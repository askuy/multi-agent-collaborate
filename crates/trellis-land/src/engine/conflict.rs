use serde::{Deserialize, Serialize};
use trellis_core::Hash;

use super::Engine;
use super::merge::MergeStep;
use crate::error::{LandError, Result};
use crate::records::{
    CONFLICT, ConflictRecord, ConflictStatus, Resolutions, SessionRecord, SessionStatus, decode,
    encode,
};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum RebaseOutcome {
    /// The session now sits on `base` with its writes merged.
    Rebased { base: Hash },
    /// Some paths could not be merged; the session's base is unchanged.
    Conflicted { conflict: String },
    /// The session's writes already landed as `changeset` (a landing whose receipt was
    /// lost); the session is now marked landed instead of re-applying them.
    AlreadyLanded { changeset: Hash },
}

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
    pub fn resolve_conflict(
        &self,
        conflict: &str,
        resolutions: &Resolutions,
    ) -> Result<RebaseOutcome> {
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
            return Err(LandError::Unresolved {
                id: conflict.to_owned(),
                paths: missing,
            });
        }
        let outcome = self.rebase_locked(session, record.theirs, resolutions)?;
        if matches!(outcome, RebaseOutcome::Rebased { .. }) {
            record.status = ConflictStatus::Resolved;
            self.meta
                .put_record(self.repo(), CONFLICT, &record.id, &encode(&record)?)?;
        }
        Ok(outcome)
    }

    fn rebase_locked(
        &self,
        mut session: SessionRecord,
        target: Hash,
        resolutions: &Resolutions,
    ) -> Result<RebaseOutcome> {
        let ctx = self.ctx();
        let scan = self.scan_since(&ctx, &session.base, &target, &session.id)?;
        if let Some(changeset) = scan.already_landed {
            // Re-merging would re-apply writes that are already on main (and undo anything
            // changed since, such as a revert); finish the session instead.
            session.status = SessionStatus::Landed { changeset };
            session.overlay.clear();
            self.save_session(&session)?;
            self.release_session_leases(&session)?;
            return Ok(RebaseOutcome::AlreadyLanded { changeset });
        }
        let step = self.merge_overlay(
            &ctx,
            &session.base,
            &target,
            &session.overlay,
            &scan.touched,
            resolutions,
            false,
        )?;
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
            MergeStep::Strict(_) => {
                unreachable!("merge_overlay only reports Strict when enforce_strict is set")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::{engine, p, text};
    use crate::records::{Outcome, Route};

    fn seed(e: &Engine, path: &str, content: &[u8]) {
        let s = e.open_session("seed", None).unwrap();
        e.write(&s.id, &p(path), Some(content)).unwrap();
        assert!(matches!(
            e.land(&s.id, "seed", "k").unwrap().outcome,
            Outcome::Landed { .. }
        ));
    }

    fn conflicted_pair(e: &Engine) -> (String, String) {
        seed(e, "doc.md", b"one\ntwo\nthree\n");
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("doc.md"), Some(b"one\nTWO-A\nthree\n"))
            .unwrap();
        e.write(&b.id, &p("doc.md"), Some(b"one\nTWO-B\nthree\n"))
            .unwrap();
        e.write(&b.id, &p("other.txt"), Some(b"untouched by a"))
            .unwrap();
        assert!(matches!(
            e.land(&a.id, "a", "k").unwrap().outcome,
            Outcome::Landed { .. }
        ));
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
        assert_eq!(
            out,
            RebaseOutcome::Rebased {
                base: e.main().unwrap()
            }
        );
        assert_eq!(
            e.conflict(&conflict).unwrap().status,
            ConflictStatus::Resolved
        );

        let r = e.land(&session, "b resolved", "k2").unwrap();
        let Outcome::Landed { route, .. } = r.outcome else {
            panic!("{:?}", r.outcome)
        };
        assert_eq!(route, Route::Parallel);
        assert_eq!(
            text(e.read_main(&p("doc.md")).unwrap()).as_deref(),
            Some("one\nTWO-A+B\nthree\n")
        );
        assert_eq!(
            text(e.read_main(&p("other.txt")).unwrap()).as_deref(),
            Some("untouched by a")
        );
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
    fn rebasing_a_session_that_already_landed_finalizes_it_instead_of_reapplying() {
        let e = engine();
        seed(&e, "doc.md", b"one\ntwo\nthree\n");
        let s = e.open_session("a", None).unwrap();
        e.write(&s.id, &p("doc.md"), Some(b"ONE\ntwo\nthree\n"))
            .unwrap();
        let Outcome::Landed { changeset, .. } = e.land(&s.id, "a", "k").unwrap().outcome else {
            panic!("expected a landing")
        };
        // Crash after the CAS: no receipt, session still open with its writes.
        e.forget_receipt(&s.id, "k").unwrap();
        let mut reopened = e.session(&s.id).unwrap();
        reopened.status = crate::SessionStatus::Open;
        reopened.overlay = e.ctx().changeset(&changeset).unwrap().changes;
        e.save_session(&reopened).unwrap();
        // Someone deliberately reverts the change.
        let revert = e.open_session("b", None).unwrap();
        e.write(&revert.id, &p("doc.md"), Some(b"one\ntwo\nthree\n"))
            .unwrap();
        e.land(&revert.id, "revert", "k").unwrap();
        let main = e.main().unwrap();

        // Instead of retrying the lost key, the client rebases: the session already landed.
        assert_eq!(
            e.rebase(&s.id).unwrap(),
            RebaseOutcome::AlreadyLanded { changeset }
        );
        assert_eq!(
            e.session(&s.id).unwrap().status,
            crate::SessionStatus::Landed { changeset }
        );
        assert!(matches!(
            e.land(&s.id, "a", "k2"),
            Err(LandError::SessionNotOpen(_))
        ));
        assert_eq!(e.main().unwrap(), main);
        assert_eq!(
            text(e.read_main(&p("doc.md")).unwrap()).as_deref(),
            Some("one\ntwo\nthree\n")
        );
    }

    #[test]
    fn rebase_moves_the_base_and_merges_clean_changes() {
        let e = engine();
        seed(&e, "doc.md", b"one\ntwo\nthree\nfour\nfive\n");
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("doc.md"), Some(b"ONE\ntwo\nthree\nfour\nfive\n"))
            .unwrap();
        e.write(&b.id, &p("doc.md"), Some(b"one\ntwo\nthree\nfour\nFIVE\n"))
            .unwrap();
        assert!(matches!(
            e.land(&a.id, "a", "k").unwrap().outcome,
            Outcome::Landed { .. }
        ));
        let main = e.main().unwrap();
        assert_eq!(
            e.rebase(&b.id).unwrap(),
            RebaseOutcome::Rebased { base: main }
        );
        assert_eq!(e.session(&b.id).unwrap().base, main);
        assert_eq!(
            text(e.read(&b.id, &p("doc.md")).unwrap()).as_deref(),
            Some("ONE\ntwo\nthree\nfour\nFIVE\n")
        );
    }
}

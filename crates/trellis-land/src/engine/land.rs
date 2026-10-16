use trellis_core::{Changeset, Hash, Provenance};
use trellis_derive::{SkipList, TreeIndex};

use super::merge::{MergeStep, collision};
use super::{Engine, MAIN, MAX_LAND_ATTEMPTS};
use crate::error::{LandError, Result};
use crate::records::{
    CONFLICT, ConflictPath, ConflictRecord, ConflictStatus, Outcome, RECEIPT, Receipt, Resolutions,
    Route, SessionRecord, SessionStatus, decode, encode,
};

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
        if let Outcome::Landed { changeset, .. } = &outcome {
            self.notify(changeset)?;
        }
        let receipt = Receipt {
            session: session.to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            outcome,
        };
        self.meta
            .put_record(&self.repo, RECEIPT, &receipt_id, &encode(&receipt)?)?;
        let status = match &receipt.outcome {
            Outcome::Landed { changeset, .. } => Some(SessionStatus::Landed {
                changeset: *changeset,
            }),
            Outcome::Unchanged { .. } => Some(SessionStatus::Closed),
            Outcome::Conflicted { .. } | Outcome::Rejected { .. } => None,
        };
        if let Some(status) = status {
            let mut record = record;
            record.status = status;
            record.overlay.clear();
            self.save_session(&record)?;
            self.release_session_leases(&record)?;
        }
        Ok(receipt)
    }

    fn land_locked(
        &self,
        session: &SessionRecord,
        message: &str,
        serialized: bool,
    ) -> Result<Outcome> {
        let ctx = self.ctx();
        for _ in 0..MAX_LAND_ATTEMPTS {
            let main = self.main()?;
            let scan = self.scan_since(&ctx, &session.base, &main, &session.id)?;
            if let Some(changeset) = scan.already_landed {
                let seq = ctx.get::<SkipList>(&changeset)?.generation;
                // The original receipt was never delivered, so there is nothing to stay
                // consistent with; stale reads are not recomputed for a recovered landing.
                return Ok(Outcome::Landed {
                    changeset,
                    seq,
                    route: Route::Parallel,
                    stale_reads: Vec::new(),
                });
            }
            // Admission comes after recovery: a lease taken after this session already
            // landed must not turn its retry into a rejection.
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
                let stale_reads = session.reads.intersection(&scan.touched).cloned().collect();
                return Ok(Outcome::Landed {
                    changeset: id,
                    seq,
                    route,
                    stale_reads,
                });
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
        self.meta
            .put_record(&self.repo, CONFLICT, &id, &encode(&record)?)?;
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
        Ok(self
            .meta
            .delete_record(&self.repo, RECEIPT, &format!("{session}/{key}"))?)
    }
}

#[cfg(test)]
mod tests {
    use trellis_derive::flatten;

    use super::*;
    use crate::engine::tests::{engine, engine_with, p, text};
    use crate::policy::{Policy, WriteMode};

    fn landed(outcome: &Outcome) -> (Hash, u64, Route) {
        match outcome {
            Outcome::Landed {
                changeset,
                seq,
                route,
                ..
            } => (*changeset, *seq, *route),
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
        assert_eq!(
            text(e.read_main(&p("a.txt")).unwrap()).as_deref(),
            Some("A")
        );
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
        assert!(matches!(
            e.land(&s.id, "m", "k2"),
            Err(LandError::SessionNotOpen(_))
        ));
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
        assert_eq!(
            files.keys().map(|k| k.as_str()).collect::<Vec<_>>(),
            ["a.txt", "b.txt"]
        );
    }

    #[test]
    fn overlapping_text_edits_compose() {
        let e = engine();
        let seed = e.open_session("seed", None).unwrap();
        e.write(
            &seed.id,
            &p("doc.md"),
            Some(b"one\ntwo\nthree\nfour\nfive\n"),
        )
        .unwrap();
        landed(&e.land(&seed.id, "seed", "k").unwrap().outcome);

        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("doc.md"), Some(b"ONE\ntwo\nthree\nfour\nfive\n"))
            .unwrap();
        e.write(&b.id, &p("doc.md"), Some(b"one\ntwo\nthree\nfour\nFIVE\n"))
            .unwrap();
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
        e.write(&seed.id, &p("doc.md"), Some(b"one\ntwo\nthree\n"))
            .unwrap();
        landed(&e.land(&seed.id, "seed", "k").unwrap().outcome);

        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("doc.md"), Some(b"one\nTWO-A\nthree\n"))
            .unwrap();
        e.write(&b.id, &p("doc.md"), Some(b"one\nTWO-B\nthree\n"))
            .unwrap();
        let main_after_a = landed(&e.land(&a.id, "a", "k").unwrap().outcome).0;

        let r = e.land(&b.id, "b", "k").unwrap();
        let Outcome::Conflicted { conflict } = &r.outcome else {
            panic!("expected Conflicted, got {:?}", r.outcome)
        };
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
        assert_eq!(
            e.land(&b.id, "b", "k").unwrap().outcome,
            Outcome::Unchanged { main }
        );
        assert_eq!(e.session(&b.id).unwrap().status, SessionStatus::Closed);
    }

    #[test]
    fn strict_paths_are_never_merged() {
        let e = engine_with(Policy::new(&[("canon/**", WriteMode::Strict)]).unwrap());
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("canon/world.json"), Some(b"{\"a\":1}"))
            .unwrap();
        e.write(&b.id, &p("canon/world.json"), Some(b"{\"b\":2}"))
            .unwrap();
        landed(&e.land(&a.id, "a", "k").unwrap().outcome);
        let r = e.land(&b.id, "b", "k").unwrap();
        assert!(
            matches!(r.outcome, Outcome::Rejected { .. }),
            "{:?}",
            r.outcome
        );
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
        assert!(matches!(
            e.land(&b.id, "b", "k").unwrap().outcome,
            Outcome::Rejected { .. }
        ));
    }

    #[test]
    fn a_lost_receipt_is_recovered_even_if_a_lease_was_taken_since() {
        let e = engine();
        let s = e.open_session("a", None).unwrap();
        e.write(&s.id, &p("a.txt"), Some(b"A")).unwrap();
        let (cs, seq, _) = landed(&e.land(&s.id, "m", "k").unwrap().outcome);

        // Crash after the CAS, then another principal leases the path before the retry.
        e.forget_receipt(&s.id, "k").unwrap();
        let mut reopened = e.session(&s.id).unwrap();
        reopened.status = SessionStatus::Open;
        reopened.overlay = e.ctx().changeset(&cs).unwrap().changes;
        e.save_session(&reopened).unwrap();
        let other = e.open_session("b", None).unwrap();
        let intent = crate::Intent {
            writes: vec![trellis_coord::Scope::parse("a.txt").unwrap()],
            exclusive: true,
            ttl_ms: 60_000,
            ..Default::default()
        };
        assert!(matches!(
            e.declare_intent(&other.id, &intent).unwrap(),
            crate::IntentOutcome::Admitted { .. }
        ));

        // The landing already happened: the retry recovers it instead of being rejected.
        let retry = e.land(&s.id, "m", "k").unwrap();
        assert_eq!(landed(&retry.outcome), (cs, seq, Route::Parallel));
        assert_eq!(e.main().unwrap(), cs);
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

    #[test]
    fn union_paths_keep_both_sides_of_concurrent_appends() {
        let e = engine_with(
            Policy::from_yaml("rules:\n  - pattern: CHANGELOG.md\n    merge: union\n").unwrap(),
        );
        let seed = e.open_session("seed", None).unwrap();
        e.write(&seed.id, &p("CHANGELOG.md"), Some(b"## Unreleased\n\n"))
            .unwrap();
        landed(&e.land(&seed.id, "seed", "k").unwrap().outcome);
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("CHANGELOG.md"), Some(b"## Unreleased\n\n- a\n"))
            .unwrap();
        e.write(&b.id, &p("CHANGELOG.md"), Some(b"## Unreleased\n\n- b\n"))
            .unwrap();
        landed(&e.land(&a.id, "a", "k").unwrap().outcome);
        let (_, _, route) = landed(&e.land(&b.id, "b", "k").unwrap().outcome);
        assert_eq!(route, Route::Composed);
        assert_eq!(
            text(e.read_main(&p("CHANGELOG.md")).unwrap()).as_deref(),
            Some("## Unreleased\n\n- b\n- a\n")
        );
    }
}

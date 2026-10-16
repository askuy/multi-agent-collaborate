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

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum IntentOutcome {
    Admitted { leases: Vec<Lease> },
    Conflict(LeaseConflict),
}

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

        let theirs = Intent {
            writes: vec![scope("book/ch3/s1.md")],
            exclusive: true,
            ttl_ms: 1000,
            ..Default::default()
        };
        let IntentOutcome::Conflict(conflict) = e.declare_intent(&b.id, &theirs).unwrap() else {
            panic!("expected a conflict")
        };
        assert_eq!(
            (conflict.holder.as_str(), conflict.intent.as_str()),
            ("alice", "rewrite chapter 3")
        );

        // Leases are advisory for writing, enforced at landing (fail-closed).
        e.write(&b.id, &p("book/ch3/s1.md"), Some(b"bob")).unwrap();
        let Outcome::Rejected { reason } = e.land(&b.id, "bob", "k").unwrap().outcome else {
            panic!("expected a rejection")
        };
        assert!(reason.contains("alice"), "{reason}");

        e.write(&a.id, &p("book/ch3/s1.md"), Some(b"alice"))
            .unwrap();
        assert!(matches!(
            e.land(&a.id, "alice", "k").unwrap().outcome,
            Outcome::Landed { .. }
        ));
        assert!(e.coord().active_leases().unwrap().is_empty());
        assert!(!matches!(
            e.land(&b.id, "bob", "k2").unwrap().outcome,
            Outcome::Rejected { .. }
        ));
    }

    #[test]
    fn intents_watch_scopes_for_notifications() {
        let e = engine();
        let watcher = e.open_session("watcher", None).unwrap();
        let watch = Intent {
            reads: vec![scope("canon/**")],
            ..Default::default()
        };
        assert!(matches!(
            e.declare_intent(&watcher.id, &watch).unwrap(),
            IntentOutcome::Admitted { .. }
        ));
        let w = e.open_session("w", None).unwrap();
        e.write(&w.id, &p("canon/world.json"), Some(b"{}")).unwrap();
        e.land(&w.id, "canon", "k").unwrap();
        assert_eq!(e.notifications(&watcher.id).unwrap().len(), 1);
    }

    #[test]
    fn closing_a_session_releases_its_leases() {
        let e = engine();
        let a = e.open_session("alice", None).unwrap();
        let intent = Intent {
            writes: vec![scope("x/**")],
            exclusive: true,
            ttl_ms: 60_000,
            ..Default::default()
        };
        e.declare_intent(&a.id, &intent).unwrap();
        assert_eq!(e.coord().active_leases().unwrap().len(), 1);
        e.close_session(&a.id).unwrap();
        assert!(e.coord().active_leases().unwrap().is_empty());
    }
}

use trellis_core::{Hash, RepoPath};
use trellis_derive::{SkipList, is_first_parent_ancestor};

use super::Engine;
use crate::error::Result;
use crate::policy::MergeStrategy;
use crate::records::{NOTE_PREFIX, Notification, SessionRecord, decode, encode};

fn note_kind(session: &str) -> String {
    format!("{NOTE_PREFIX}{session}")
}

fn watches(session: &SessionRecord, path: &RepoPath) -> bool {
    session.reads.contains(path) || session.watch.iter().any(|scope| scope.covers(path))
}

impl Engine {
    /// Records a notification for every open session that read or watches a path
    /// `changeset` changed and whose base does not include it yet (spec §4.3, CoAgent:
    /// notify, do not lock or abort). Paths merged with `merge: union` are left out: concurrent
    /// appends there always keep every side, so readers have nothing to act on, and with many
    /// agents appending to one changelog the notes would drown the ones that matter.
    /// Idempotent: re-running keeps existing notifications.
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
            let paths: Vec<RepoPath> = cs
                .changes
                .keys()
                .filter(|p| watches(&session, p) && self.policy.merge(p) != MergeStrategy::Union)
                .cloned()
                .collect();
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
                self.meta
                    .put_record(&self.repo, &kind, &id, &encode(&note)?)?;
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
        Ok(self
            .all_notifications(session)?
            .into_iter()
            .filter(|n| !n.acked)
            .collect())
    }

    /// Acknowledges every notification up to and including `through_seq`; returns how many.
    pub fn ack_notifications(&self, session: &str, through_seq: u64) -> Result<usize> {
        let kind = note_kind(session);
        let mut acked = 0;
        for mut note in self.notifications(session)? {
            if note.seq <= through_seq {
                note.acked = true;
                self.meta.put_record(
                    &self.repo,
                    &kind,
                    &format!("{:020}", note.seq),
                    &encode(&note)?,
                )?;
                acked += 1;
            }
        }
        Ok(acked)
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::tests::{engine, engine_with, p, seed};
    use crate::policy::Policy;
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
        let Outcome::Landed { changeset, .. } =
            e.land(&writer.id, "update doc", "k").unwrap().outcome
        else {
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
    fn union_merged_paths_do_not_notify() {
        let e = engine_with(
            Policy::from_yaml("rules: [{pattern: \"CHANGELOG.md\", merge: union}]").unwrap(),
        );
        seed(&e, "CHANGELOG.md", b"## Unreleased\n");
        let reader = e.open_session("reader", None).unwrap();
        e.read(&reader.id, &p("CHANGELOG.md")).unwrap();
        e.read(&reader.id, &p("doc.md")).unwrap();
        let writer = e.open_session("writer", None).unwrap();
        e.write(
            &writer.id,
            &p("CHANGELOG.md"),
            Some(b"## Unreleased\n- w\n"),
        )
        .unwrap();
        e.land(&writer.id, "changelog only", "k1").unwrap();
        assert!(e.notifications(&reader.id).unwrap().is_empty());

        let writer = e.open_session("writer", None).unwrap();
        e.write(
            &writer.id,
            &p("CHANGELOG.md"),
            Some(b"## Unreleased\n- w\n- x\n"),
        )
        .unwrap();
        e.write(&writer.id, &p("doc.md"), Some(b"v2")).unwrap();
        e.land(&writer.id, "doc and changelog", "k2").unwrap();
        let notes = e.notifications(&reader.id).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].paths, vec![p("doc.md")]);
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

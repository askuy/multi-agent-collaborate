use serde::{Deserialize, Serialize};
use trellis_core::RepoPath;

use crate::coord::Coord;
use crate::error::{CoordError, Result};
use crate::scope::Scope;

const ENTRY: &str = "log";
const LOG_META: &str = "log-meta";
const CURSOR: &str = "log-cursor";
const LOG_LOCK: &str = "log";

/// The kinds of shared-context entries (spec §4.5, DeLM FACT/FAIL/summary).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    /// A finding supported by evidence.
    Fact,
    /// A contradicted approach or a defect in someone's result; open until resolved.
    Fail,
    /// The result of a finished task.
    Summary,
    /// An agreed decision.
    Decision,
    Note,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryStatus {
    Open,
    Resolved,
}

/// What an author supplies; the log assigns the sequence number.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NewEntry {
    pub kind: Option<EntryKind>,
    pub topics: Vec<String>,
    pub scope: Vec<Scope>,
    pub body: String,
    pub evidence: Vec<String>,
}

/// An immutable log entry; only `status` and `resolution` change, when a FAIL is resolved.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub kind: EntryKind,
    pub author: String,
    pub topics: Vec<String>,
    pub scope: Vec<Scope>,
    pub body: String,
    pub evidence: Vec<String>,
    pub status: EntryStatus,
    pub resolution: Option<String>,
    pub created_at_ms: i64,
}

fn entry_id(seq: u64) -> String {
    format!("{seq:020}")
}

impl Coord {
    /// Appends an entry with the next sequence number. FAIL entries start open.
    pub fn append(&self, author: &str, new: NewEntry) -> Result<Entry> {
        let _guard = self.locks.lock(&[LOG_LOCK.to_owned()]);
        let seq = self.load::<u64>(LOG_META, "seq")?.unwrap_or(0) + 1;
        let kind = new.kind.unwrap_or(EntryKind::Note);
        let entry = Entry {
            seq,
            kind,
            author: author.to_owned(),
            topics: new.topics,
            scope: new.scope,
            body: new.body,
            evidence: new.evidence,
            status: if kind == EntryKind::Fail {
                EntryStatus::Open
            } else {
                EntryStatus::Resolved
            },
            resolution: None,
            created_at_ms: self.env.now_ms(),
        };
        self.save(ENTRY, &entry_id(seq), &entry)?;
        self.save(LOG_META, "seq", &seq)?;
        Ok(entry)
    }

    /// Entries after `after_seq`, oldest first, at most `limit`.
    pub fn entries(&self, after_seq: u64, limit: usize) -> Result<Vec<Entry>> {
        Ok(self
            .load_all::<Entry>(ENTRY)?
            .into_iter()
            .filter(|e| e.seq > after_seq)
            .take(limit)
            .collect())
    }

    /// What `reader` has not seen yet, filtered by `topics` (empty means every entry). The
    /// cursor moves past everything new, including filtered-out entries, so nothing is ever
    /// delivered twice (DeLM: a read shows only what is new since the last read).
    pub fn read_new(&self, reader: &str, topics: &[String]) -> Result<Vec<Entry>> {
        let _guard = self.locks.lock(&[format!("cursor:{reader}")]);
        let cursor: u64 = self.load(CURSOR, reader)?.unwrap_or(0);
        let fresh = self.entries(cursor, usize::MAX)?;
        if let Some(last) = fresh.last() {
            self.save(CURSOR, reader, &last.seq)?;
        }
        Ok(fresh
            .into_iter()
            .filter(|e| topics.is_empty() || e.topics.iter().any(|t| topics.contains(t)))
            .collect())
    }

    pub fn resolve_entry(&self, seq: u64, by: &str, note: &str) -> Result<Entry> {
        let _guard = self.locks.lock(&[LOG_LOCK.to_owned()]);
        let mut entry: Entry = self
            .load(ENTRY, &entry_id(seq))?
            .ok_or(CoordError::EntryNotFound(seq))?;
        entry.status = EntryStatus::Resolved;
        entry.resolution = Some(format!("{by}: {note}"));
        self.save(ENTRY, &entry_id(seq), &entry)?;
        Ok(entry)
    }

    /// Open FAIL entries; with `path`, only those whose scope covers it (or that have no scope).
    pub fn open_fails(&self, path: Option<&RepoPath>) -> Result<Vec<Entry>> {
        Ok(self
            .entries(0, usize::MAX)?
            .into_iter()
            .filter(|e| e.kind == EntryKind::Fail && e.status == EntryStatus::Open)
            .filter(|e| {
                path.is_none_or(|p| e.scope.is_empty() || e.scope.iter().any(|s| s.covers(p)))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::tests::coord;

    fn entry(kind: EntryKind, topic: &str, body: &str) -> NewEntry {
        NewEntry {
            kind: Some(kind),
            topics: vec![topic.into()],
            body: body.into(),
            ..Default::default()
        }
    }

    fn seqs(entries: &[Entry]) -> Vec<u64> {
        entries.iter().map(|e| e.seq).collect()
    }

    #[test]
    fn reads_return_only_entries_not_seen_before() {
        let (c, _) = coord();
        let first = c
            .append("a", entry(EntryKind::Fact, "ch1", "alice is injured"))
            .unwrap();
        assert_eq!((first.seq, first.status), (1, EntryStatus::Resolved));
        c.append("b", entry(EntryKind::Fail, "ch2", "timeline broken"))
            .unwrap();
        assert_eq!(seqs(&c.read_new("reader", &[]).unwrap()), [1, 2]);
        assert!(c.read_new("reader", &[]).unwrap().is_empty());
        c.append("a", entry(EntryKind::Note, "ch1", "later"))
            .unwrap();
        assert_eq!(seqs(&c.read_new("reader", &[]).unwrap()), [3]);
        // Another reader has its own cursor.
        assert_eq!(seqs(&c.read_new("other", &[]).unwrap()), [1, 2, 3]);
    }

    #[test]
    fn topic_filters_skip_entries_without_redelivering_them() {
        let (c, _) = coord();
        c.append("a", entry(EntryKind::Fact, "ch1", "x")).unwrap();
        c.append("a", entry(EntryKind::Fact, "ch2", "y")).unwrap();
        let only_ch1 = c.read_new("r", &["ch1".into()]).unwrap();
        assert_eq!(seqs(&only_ch1), [1]);
        assert!(c.read_new("r", &[]).unwrap().is_empty());
        assert_eq!(seqs(&c.entries(0, 10).unwrap()), [1, 2]);
        assert_eq!(seqs(&c.entries(1, 10).unwrap()), [2]);
        assert_eq!(seqs(&c.entries(0, 1).unwrap()), [1]);
    }

    #[test]
    fn fails_stay_open_until_resolved_and_filter_by_scope() {
        let (c, _) = coord();
        let fail = c
            .append(
                "checker",
                NewEntry {
                    kind: Some(EntryKind::Fail),
                    scope: vec![Scope::parse("book/ch1/**").unwrap()],
                    body: "alice lifts with an injured arm".into(),
                    evidence: vec!["changeset abc".into()],
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(fail.status, EntryStatus::Open);
        let inside = RepoPath::parse("book/ch1/s2.md").unwrap();
        let outside = RepoPath::parse("book/ch2/s1.md").unwrap();
        assert_eq!(c.open_fails(Some(&inside)).unwrap().len(), 1);
        assert!(c.open_fails(Some(&outside)).unwrap().is_empty());
        assert_eq!(c.open_fails(None).unwrap().len(), 1);
        let resolved = c.resolve_entry(fail.seq, "writer", "fixed in s2").unwrap();
        assert_eq!(resolved.status, EntryStatus::Resolved);
        assert_eq!(resolved.resolution.as_deref(), Some("writer: fixed in s2"));
        assert!(c.open_fails(None).unwrap().is_empty());
        assert!(matches!(
            c.resolve_entry(99, "x", "y"),
            Err(CoordError::EntryNotFound(99))
        ));
    }
}

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, params};
use trellis_core::{Changeset, Hash};

use crate::error::{Result, StoreError};
use crate::meta::{MetaStore, RefState};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS changesets (
    repo TEXT NOT NULL,
    id BLOB NOT NULL,
    body BLOB NOT NULL,
    PRIMARY KEY (repo, id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS refs (
    repo TEXT NOT NULL,
    name TEXT NOT NULL,
    target BLOB NOT NULL,
    version INTEGER NOT NULL,
    PRIMARY KEY (repo, name)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS derived (
    repo TEXT NOT NULL,
    cs BLOB NOT NULL,
    name TEXT NOT NULL,
    version INTEGER NOT NULL,
    value BLOB NOT NULL,
    PRIMARY KEY (repo, cs, name, version)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS records (
    repo TEXT NOT NULL,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    body BLOB NOT NULL,
    PRIMARY KEY (repo, kind, id)
) WITHOUT ROWID;
";

/// A [`MetaStore`] on one SQLite database.
pub struct SqliteMetaStore {
    conn: Mutex<Connection>,
}

impl SqliteMetaStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| StoreError::Poisoned)
    }
}

impl MetaStore for SqliteMetaStore {
    fn put_changeset(&self, repo: &str, cs: &Changeset) -> Result<Hash> {
        let body = cs.encode();
        let id = Hash::of(&body);
        self.conn()?.execute(
            "INSERT OR IGNORE INTO changesets (repo, id, body) VALUES (?1, ?2, ?3)",
            params![repo, id.as_bytes().as_slice(), body],
        )?;
        Ok(id)
    }

    fn get_changeset(&self, repo: &str, id: &Hash) -> Result<Option<Changeset>> {
        let body: Option<Vec<u8>> = self
            .conn()?
            .query_row(
                "SELECT body FROM changesets WHERE repo = ?1 AND id = ?2",
                params![repo, id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(body) = body else { return Ok(None) };
        if Hash::of(&body) != *id {
            return Err(StoreError::Corrupt(*id));
        }
        Ok(Some(Changeset::decode(&body)?))
    }

    fn get_ref(&self, repo: &str, name: &str) -> Result<Option<RefState>> {
        let row: Option<(Vec<u8>, i64)> = self
            .conn()?
            .query_row(
                "SELECT target, version FROM refs WHERE repo = ?1 AND name = ?2",
                params![repo, name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(target, version)| {
            Ok(RefState {
                target: Hash::from_slice(&target)?,
                version: version as u64,
            })
        })
        .transpose()
    }

    fn cas_ref(&self, repo: &str, name: &str, expected: Option<Hash>, new: Hash) -> Result<bool> {
        let conn = self.conn()?;
        let changed = match expected {
            None => conn.execute(
                "INSERT OR IGNORE INTO refs (repo, name, target, version) VALUES (?1, ?2, ?3, 1)",
                params![repo, name, new.as_bytes().as_slice()],
            )?,
            Some(old) => conn.execute(
                "UPDATE refs SET target = ?3, version = version + 1
                 WHERE repo = ?1 AND name = ?2 AND target = ?4",
                params![
                    repo,
                    name,
                    new.as_bytes().as_slice(),
                    old.as_bytes().as_slice()
                ],
            )?,
        };
        Ok(changed == 1)
    }

    fn put_derived(
        &self,
        repo: &str,
        cs: &Hash,
        name: &str,
        version: u32,
        value: &[u8],
    ) -> Result<()> {
        self.conn()?.execute(
            "INSERT OR IGNORE INTO derived (repo, cs, name, version, value) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![repo, cs.as_bytes().as_slice(), name, version, value],
        )?;
        Ok(())
    }

    fn get_derived(
        &self,
        repo: &str,
        cs: &Hash,
        name: &str,
        version: u32,
    ) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn()?
            .query_row(
                "SELECT value FROM derived WHERE repo = ?1 AND cs = ?2 AND name = ?3 AND version = ?4",
                params![repo, cs.as_bytes().as_slice(), name, version],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn put_record(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO records (repo, kind, id, body) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (repo, kind, id) DO UPDATE SET body = excluded.body",
            params![repo, kind, id, body],
        )?;
        Ok(())
    }

    fn put_record_if_absent(&self, repo: &str, kind: &str, id: &str, body: &[u8]) -> Result<bool> {
        let inserted = self.conn()?.execute(
            "INSERT OR IGNORE INTO records (repo, kind, id, body) VALUES (?1, ?2, ?3, ?4)",
            params![repo, kind, id, body],
        )?;
        Ok(inserted == 1)
    }

    fn get_record(&self, repo: &str, kind: &str, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn()?
            .query_row(
                "SELECT body FROM records WHERE repo = ?1 AND kind = ?2 AND id = ?3",
                params![repo, kind, id],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn list_records(&self, repo: &str, kind: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT id, body FROM records WHERE repo = ?1 AND kind = ?2 ORDER BY id")?;
        let rows = stmt.query_map(params![repo, kind], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn delete_record(&self, repo: &str, kind: &str, id: &str) -> Result<()> {
        self.conn()?.execute(
            "DELETE FROM records WHERE repo = ?1 AND kind = ?2 AND id = ?3",
            params![repo, kind, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use trellis_core::Provenance;

    use super::*;

    fn cs(message: &str, parents: Vec<Hash>) -> Changeset {
        Changeset {
            parents,
            changes: BTreeMap::new(),
            provenance: Provenance {
                principal: "t".into(),
                ..Default::default()
            },
            message: message.into(),
            created_at_ms: 0,
        }
    }

    #[test]
    fn changesets_round_trip_idempotently_per_repo() {
        let s = SqliteMetaStore::open_in_memory().unwrap();
        let c = cs("a", vec![]);
        let id = s.put_changeset("r", &c).unwrap();
        assert_eq!(id, c.id());
        assert_eq!(s.put_changeset("r", &c).unwrap(), id);
        assert_eq!(s.get_changeset("r", &id).unwrap(), Some(c));
        assert_eq!(s.get_changeset("other", &id).unwrap(), None);
    }

    #[test]
    fn cas_ref_only_moves_from_the_expected_target() {
        let s = SqliteMetaStore::open_in_memory().unwrap();
        let (a, b) = (Hash::of(b"a"), Hash::of(b"b"));
        assert_eq!(s.get_ref("r", "main").unwrap(), None);
        assert!(s.cas_ref("r", "main", None, a).unwrap());
        assert!(!s.cas_ref("r", "main", None, b).unwrap());
        assert_eq!(
            s.get_ref("r", "main").unwrap(),
            Some(RefState {
                target: a,
                version: 1
            })
        );
        assert!(!s.cas_ref("r", "main", Some(b), b).unwrap());
        assert!(s.cas_ref("r", "main", Some(a), b).unwrap());
        assert_eq!(
            s.get_ref("r", "main").unwrap(),
            Some(RefState {
                target: b,
                version: 2
            })
        );
    }

    #[test]
    fn concurrent_cas_has_exactly_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        let s = Arc::new(SqliteMetaStore::open(dir.path().join("meta.db")).unwrap());
        let start = Hash::of(b"start");
        assert!(s.cas_ref("r", "main", None, start).unwrap());
        let handles: Vec<_> = (0..8u8)
            .map(|i| {
                let s = Arc::clone(&s);
                std::thread::spawn(move || {
                    s.cas_ref("r", "main", Some(start), Hash::of(&[i])).unwrap()
                })
            })
            .collect();
        let winners = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
        assert_eq!(s.get_ref("r", "main").unwrap().unwrap().version, 2);
    }

    #[test]
    fn derived_values_are_keyed_by_name_and_version_and_first_write_wins() {
        let s = SqliteMetaStore::open_in_memory().unwrap();
        let id = Hash::of(b"cs");
        s.put_derived("r", &id, "tree", 1, b"v1").unwrap();
        s.put_derived("r", &id, "tree", 1, b"other").unwrap();
        assert_eq!(
            s.get_derived("r", &id, "tree", 1).unwrap().as_deref(),
            Some(&b"v1"[..])
        );
        assert_eq!(s.get_derived("r", &id, "tree", 2).unwrap(), None);
        assert_eq!(s.get_derived("r", &id, "path_set", 1).unwrap(), None);
    }

    #[test]
    fn file_store_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.db");
        let c = cs("durable", vec![]);
        {
            let s = SqliteMetaStore::open(&path).unwrap();
            let id = s.put_changeset("r", &c).unwrap();
            assert!(s.cas_ref("r", "main", None, id).unwrap());
        }
        let s = SqliteMetaStore::open(&path).unwrap();
        let target = s.get_ref("r", "main").unwrap().unwrap().target;
        assert_eq!(s.get_changeset("r", &target).unwrap(), Some(c));
    }

    #[test]
    fn records_upsert_insert_once_and_list_in_id_order() {
        let s = SqliteMetaStore::open_in_memory().unwrap();
        assert_eq!(s.get_record("r", "session", "b").unwrap(), None);
        s.put_record("r", "session", "b", b"1").unwrap();
        s.put_record("r", "session", "b", b"2").unwrap();
        s.put_record("r", "session", "a", b"x").unwrap();
        s.put_record("r", "receipt", "a", b"y").unwrap();
        s.put_record("other", "session", "c", b"z").unwrap();
        assert_eq!(
            s.get_record("r", "session", "b").unwrap().as_deref(),
            Some(&b"2"[..])
        );
        assert_eq!(
            s.list_records("r", "session").unwrap(),
            vec![
                ("a".to_owned(), b"x".to_vec()),
                ("b".to_owned(), b"2".to_vec())
            ]
        );
        assert!(s.put_record_if_absent("r", "receipt", "new", b"1").unwrap());
        assert!(!s.put_record_if_absent("r", "receipt", "new", b"2").unwrap());
        assert_eq!(
            s.get_record("r", "receipt", "new").unwrap().as_deref(),
            Some(&b"1"[..])
        );
        s.delete_record("r", "receipt", "new").unwrap();
        assert_eq!(s.get_record("r", "receipt", "new").unwrap(), None);
    }
}

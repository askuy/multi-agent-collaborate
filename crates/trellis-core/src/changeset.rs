use std::collections::BTreeMap;

use crate::codec::{Reader, Writer};
use crate::error::{CoreError, Result};
use crate::hash::Hash;
use crate::kind::FileKind;
use crate::path::RepoPath;

const MAGIC: &[u8] = b"TRLS-CS\x01";

/// A content-addressed blob and its length.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlobRef {
    pub hash: Hash,
    pub size: u64,
}

/// One file's change relative to the first parent.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum FileChange {
    Upsert { blob: BlobRef, kind: FileKind },
    Delete,
}

/// Who made a change and in which context (spec §2, from Libra traces).
#[derive(Clone, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Provenance {
    pub principal: String,
    pub session: Option<String>,
    pub task: Option<String>,
    pub model: Option<String>,
    pub trace: Option<Hash>,
}

/// A write-ahead-log entry: the files changed relative to the first parent, with
/// content-addressed pointers (spec §4.1, from Scion and Mononoke).
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Changeset {
    pub parents: Vec<Hash>,
    pub changes: BTreeMap<RepoPath, FileChange>,
    pub provenance: Provenance,
    pub message: String,
    pub created_at_ms: i64,
}

impl BlobRef {
    pub fn of(data: &[u8]) -> Self {
        Self {
            hash: Hash::of(data),
            size: data.len() as u64,
        }
    }
}

impl Changeset {
    pub fn first_parent(&self) -> Option<Hash> {
        self.parents.first().copied()
    }

    /// The changeset id: BLAKE3 of the canonical encoding.
    pub fn id(&self) -> Hash {
        Hash::of(&self.encode())
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(MAGIC);
        w.count(self.parents.len());
        for parent in &self.parents {
            w.hash(parent);
        }
        w.count(self.changes.len());
        for (path, change) in &self.changes {
            w.str(path.as_str());
            match change {
                FileChange::Upsert { blob, kind } => {
                    w.u8(1);
                    w.hash(&blob.hash);
                    w.u64(blob.size);
                    w.u8(kind.to_u8());
                }
                FileChange::Delete => w.u8(2),
            }
        }
        let p = &self.provenance;
        w.str(&p.principal);
        w.opt_str(p.session.as_deref());
        w.opt_str(p.task.as_deref());
        w.opt_str(p.model.as_deref());
        w.opt_hash(p.trace.as_ref());
        w.str(&self.message);
        w.i64(self.created_at_ms);
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, MAGIC)?;
        let parent_count = r.count()?;
        let mut parents = Vec::new();
        for _ in 0..parent_count {
            parents.push(r.hash()?);
        }
        let change_count = r.count()?;
        let mut changes = BTreeMap::new();
        let mut previous: Option<RepoPath> = None;
        for _ in 0..change_count {
            let path = RepoPath::parse(&r.string()?)?;
            if previous.as_ref().is_some_and(|prev| prev >= &path) {
                return Err(CoreError::Decode("changes not strictly sorted"));
            }
            let change = match r.u8()? {
                1 => {
                    let hash = r.hash()?;
                    let size = r.u64()?;
                    let kind = FileKind::from_u8(r.u8()?)?;
                    FileChange::Upsert {
                        blob: BlobRef { hash, size },
                        kind,
                    }
                }
                2 => FileChange::Delete,
                _ => return Err(CoreError::Decode("unknown change tag")),
            };
            previous = Some(path.clone());
            changes.insert(path, change);
        }
        let provenance = Provenance {
            principal: r.string()?,
            session: r.opt_string()?,
            task: r.opt_string()?,
            model: r.opt_string()?,
            trace: r.opt_hash()?,
        };
        let message = r.string()?;
        let created_at_ms = r.i64()?;
        r.finish()?;
        Ok(Self {
            parents,
            changes,
            provenance,
            message,
            created_at_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn sample() -> Changeset {
        let mut changes = BTreeMap::new();
        changes.insert(
            RepoPath::parse("docs/a.md").unwrap(),
            FileChange::Upsert {
                blob: BlobRef::of(b"hello"),
                kind: FileKind::Markdown,
            },
        );
        changes.insert(RepoPath::parse("old.txt").unwrap(), FileChange::Delete);
        Changeset {
            parents: vec![Hash::of(b"parent")],
            changes,
            provenance: Provenance {
                principal: "agent-7".into(),
                session: Some("s1".into()),
                task: Some("T-12".into()),
                model: Some("claude-opus-5".into()),
                trace: Some(Hash::of(b"trace")),
            },
            message: "edit docs".into(),
            created_at_ms: 1_760_000_000_000,
        }
    }

    #[test]
    fn round_trips_and_id_is_hash_of_encoding() {
        let cs = sample();
        let bytes = cs.encode();
        assert_eq!(Changeset::decode(&bytes).unwrap(), cs);
        assert_eq!(cs.id(), Hash::of(&bytes));
        assert_eq!(cs.first_parent(), Some(Hash::of(b"parent")));
    }

    #[test]
    fn id_depends_on_every_field() {
        let base = sample();
        let mut other = base.clone();
        other.message.push('!');
        assert_ne!(base.id(), other.id());
        let mut other = base.clone();
        other.provenance.session = None;
        assert_ne!(base.id(), other.id());
        assert_eq!(base.id(), sample().id());
    }

    #[test]
    fn rejects_unsorted_changes_and_trailing_bytes() {
        let mut w = Writer::new(MAGIC);
        w.count(0);
        w.count(2);
        w.str("b");
        w.u8(2);
        w.str("a");
        w.u8(2);
        w.str("p");
        w.opt_str(None);
        w.opt_str(None);
        w.opt_str(None);
        w.opt_hash(None);
        w.str("");
        w.i64(0);
        assert_eq!(
            Changeset::decode(&w.finish()),
            Err(CoreError::Decode("changes not strictly sorted"))
        );

        let mut bytes = sample().encode();
        bytes.push(0);
        assert_eq!(
            Changeset::decode(&bytes),
            Err(CoreError::Decode("trailing bytes"))
        );
    }

    #[test]
    fn serializes_to_readable_json_and_back() {
        let cs = sample();
        let json = serde_json::to_value(&cs).unwrap();
        assert_eq!(
            json["parents"][0],
            serde_json::json!(Hash::of(b"parent").to_hex())
        );
        assert_eq!(json["changes"]["docs/a.md"]["op"], "upsert");
        assert_eq!(json["changes"]["docs/a.md"]["kind"], "markdown");
        assert_eq!(json["changes"]["old.txt"]["op"], "delete");
        let back: Changeset = serde_json::from_value(json).unwrap();
        assert_eq!(back, cs);

        let bad = serde_json::json!({"op": "delete"});
        assert!(serde_json::from_value::<FileChange>(bad).is_ok());
        assert!(serde_json::from_str::<RepoPath>("\"../x\"").is_err());
        assert!(serde_json::from_str::<Hash>("\"abc\"").is_err());
    }

    fn arb_change() -> impl Strategy<Value = FileChange> {
        prop_oneof![
            (any::<[u8; 32]>(), any::<u64>(), 1u8..=5).prop_map(|(h, size, k)| {
                FileChange::Upsert {
                    blob: BlobRef {
                        hash: Hash::from_bytes(h),
                        size,
                    },
                    kind: FileKind::from_u8(k).unwrap(),
                }
            }),
            Just(FileChange::Delete),
        ]
    }

    fn arb_changeset() -> impl Strategy<Value = Changeset> {
        (
            prop::collection::vec(any::<[u8; 32]>().prop_map(Hash::from_bytes), 0..3),
            prop::collection::btree_map(
                "[a-z]{1,3}(/[a-z]{1,3}){0,2}".prop_map(|s| RepoPath::parse(&s).unwrap()),
                arb_change(),
                0..8,
            ),
            ".{0,12}",
            proptest::option::of("[a-z]{0,6}"),
            any::<i64>(),
        )
            .prop_map(
                |(parents, changes, message, session, created_at_ms)| Changeset {
                    parents,
                    changes,
                    provenance: Provenance {
                        principal: "agent".into(),
                        session,
                        ..Default::default()
                    },
                    message,
                    created_at_ms,
                },
            )
    }

    proptest! {
        #[test]
        fn encode_decode_round_trips(cs in arb_changeset()) {
            let bytes = cs.encode();
            prop_assert_eq!(Changeset::decode(&bytes).unwrap(), cs.clone());
            prop_assert_eq!(Hash::of(&bytes), cs.id());
        }
    }
}

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use trellis_coord::Scope;
use trellis_core::{BlobRef, FileChange, FileKind, Hash, RepoPath};

use crate::error::Result;

pub(crate) const SESSION: &str = "session";
pub(crate) const RECEIPT: &str = "receipt";
pub(crate) const CONFLICT: &str = "conflict";
pub(crate) const NOTE_PREFIX: &str = "note:";

/// A session's unlanded writes, relative to its base.
pub type Overlay = BTreeMap<RepoPath, FileChange>;

/// Final contents for conflicting paths: `Some(bytes)` to write, `None` to delete.
pub type Resolutions = BTreeMap<RepoPath, Option<Vec<u8>>>;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum SessionStatus {
    Open,
    Landed { changeset: Hash },
    Closed,
}

/// One agent's workspace for one task: a base on `main` plus unlanded writes.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub principal: String,
    pub task: Option<String>,
    pub base: Hash,
    pub status: SessionStatus,
    pub overlay: Overlay,
    pub created_at_ms: i64,
    /// Paths read from the base; a landing that changes one notifies this session.
    #[serde(default)]
    pub reads: BTreeSet<RepoPath>,
    /// Scopes declared in intents; also trigger notifications.
    #[serde(default)]
    pub watch: Vec<Scope>,
    /// Leases acquired through intents, released when the session lands or closes.
    #[serde(default)]
    pub leases: Vec<String>,
}

/// How a landing reached `main`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Route {
    /// No path the session wrote changed on `main` since its base.
    Parallel,
    /// At least one path was merged with changes on `main`.
    Composed,
    /// Landed while holding a serialize lock from the policy.
    Serialized,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum Outcome {
    Landed {
        changeset: Hash,
        seq: u64,
        route: Route,
        /// Paths this session read that changed on `main` after its base (spec §4.3).
        #[serde(default)]
        stale_reads: Vec<RepoPath>,
    },
    /// Everything the session wrote was already on `main`; nothing new landed.
    Unchanged { main: Hash },
    /// Some paths could not be merged; see the conflict record. The session stays open.
    Conflicted { conflict: String },
    /// Fail-closed: the policy or the tree shape forbids this landing. The session stays open.
    Rejected { reason: String },
}

/// The stored result of one `land` call, returned again for a retry with the same key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub session: String,
    pub idempotency_key: String,
    pub outcome: Outcome,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConflictPath {
    pub path: RepoPath,
    pub kind: FileKind,
    pub base: Option<BlobRef>,
    pub ours: Option<BlobRef>,
    pub theirs: Option<BlobRef>,
    pub reason: String,
    pub detail: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictStatus {
    Open,
    Resolved,
}

/// A first-class conflict (spec §4.6): the session's base, the `main` it failed to merge
/// with, and the three versions of every unmergeable path.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub id: String,
    pub session: String,
    pub base: Hash,
    pub theirs: Hash,
    pub paths: Vec<ConflictPath>,
    pub status: ConflictStatus,
}

/// A landing that touched something a session read or watches (spec §4.3).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Notification {
    pub changeset: Hash,
    pub seq: u64,
    pub author: String,
    pub author_session: Option<String>,
    pub message: String,
    /// The changed paths this session read or watches.
    pub paths: Vec<RepoPath>,
    pub acked: bool,
}

/// One file as a session sees it: its base with its own writes on top.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: RepoPath,
    pub blob: BlobRef,
    pub kind: FileKind,
    /// Written in this session and not yet landed.
    pub changed: bool,
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    Ok(serde_json::from_slice(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip_through_json() {
        let receipt = Receipt {
            session: "s-1".into(),
            idempotency_key: "k".into(),
            outcome: Outcome::Landed {
                changeset: Hash::of(b"c"),
                seq: 3,
                route: Route::Composed,
                stale_reads: vec![],
            },
        };
        let back: Receipt = decode(&encode(&receipt).unwrap()).unwrap();
        assert_eq!(back, receipt);
        let json: serde_json::Value = serde_json::from_slice(&encode(&receipt).unwrap()).unwrap();
        assert_eq!(json["outcome"]["outcome"], "landed");
        assert_eq!(json["outcome"]["route"], "composed");

        let session = SessionRecord {
            id: "s-1".into(),
            principal: "agent".into(),
            task: None,
            base: Hash::of(b"b"),
            status: SessionStatus::Landed {
                changeset: Hash::of(b"c"),
            },
            overlay: Overlay::new(),
            created_at_ms: 5,
            reads: Default::default(),
            watch: vec![],
            leases: vec![],
        };
        let back: SessionRecord = decode(&encode(&session).unwrap()).unwrap();
        assert_eq!(back, session);
    }
}

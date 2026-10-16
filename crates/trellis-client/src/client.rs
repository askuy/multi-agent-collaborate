use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::{Client as Http, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use trellis_coord::{ClaimOutcome, Entry, Lease, NewEntry, Scope, Task};
use trellis_core::{Changeset, Hash};
use trellis_land::{
    ConflictRecord, FileEntry, Intent, IntentOutcome, Notification, RebaseOutcome, Receipt,
    SessionRecord,
};

use crate::error::{ClientError, Result};

/// One changeset of `main` with its sequence number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangesetEntry {
    pub seq: u64,
    pub id: Hash,
    pub changeset: Changeset,
}

#[derive(Deserialize)]
struct RepoInfo {
    main: Hash,
}

/// A client for one repository on one server.
#[derive(Clone)]
pub struct Client {
    base: Url,
    repo: String,
    http: Http,
}

fn send(request: RequestBuilder) -> Result<Response> {
    let response = request.send()?;
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    let body: serde_json::Value = response.json().unwrap_or_default();
    Err(ClientError::Api {
        status,
        code: body["error"].as_str().unwrap_or("unknown").to_owned(),
        message: body["message"].as_str().unwrap_or_default().to_owned(),
    })
}

fn json<T: DeserializeOwned>(request: RequestBuilder) -> Result<T> {
    Ok(send(request)?.json()?)
}

fn bytes_or_none(request: RequestBuilder) -> Result<Option<Vec<u8>>> {
    match send(request) {
        Ok(response) => Ok(Some(response.bytes()?.to_vec())),
        Err(ClientError::Api {
            status: 404, code, ..
        }) if code == "no-such-file" => Ok(None),
        Err(e) => Err(e),
    }
}

impl Client {
    pub fn new(server: &str, repo: &str) -> Result<Self> {
        let base =
            Url::parse(server).map_err(|e| ClientError::InvalidServer(format!("{server}: {e}")))?;
        let mut builder = Http::builder().timeout(Duration::from_secs(120));
        // A local server must never be reached through a proxy from the environment
        // (HTTP_PROXY without a matching NO_PROXY would intercept it).
        let loopback = matches!(
            base.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        );
        if loopback {
            builder = builder.no_proxy();
        }
        let http = builder.build()?;
        Ok(Self {
            base,
            repo: repo.to_owned(),
            http,
        })
    }

    pub fn server(&self) -> &str {
        self.base.as_str()
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }

    fn url(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|()| ClientError::InvalidServer(self.base.to_string()))?
            .pop_if_empty()
            .push("repos")
            .push(&self.repo)
            .extend(segments);
        Ok(url)
    }

    fn file_url(&self, prefix: &[&str], path: &str) -> Result<Url> {
        let mut segments = prefix.to_vec();
        segments.extend(path.split('/'));
        self.url(&segments)
    }

    /// Creates the repository if needed; returns `main`.
    pub fn init(&self) -> Result<Hash> {
        Ok(json::<RepoInfo>(self.http.post(self.url(&[])?))?.main)
    }

    pub fn main(&self) -> Result<Hash> {
        Ok(json::<RepoInfo>(self.http.get(self.url(&[])?))?.main)
    }

    pub fn read_main(&self, path: &str) -> Result<Option<Vec<u8>>> {
        bytes_or_none(self.http.get(self.file_url(&["main", "files"], path)?))
    }

    pub fn blob(&self, hash: &Hash) -> Result<Vec<u8>> {
        Ok(send(self.http.get(self.url(&["blobs", &hash.to_hex()])?))?
            .bytes()?
            .to_vec())
    }

    pub fn changesets_since(&self, after_seq: u64) -> Result<Vec<ChangesetEntry>> {
        json(
            self.http
                .get(self.url(&["changesets"])?)
                .query(&[("after_seq", after_seq)]),
        )
    }

    pub fn open_session(&self, principal: &str, task: Option<&str>) -> Result<SessionRecord> {
        json(
            self.http
                .post(self.url(&["sessions"])?)
                .json(&json!({ "principal": principal, "task": task })),
        )
    }

    pub fn sessions(&self, open_only: bool) -> Result<Vec<SessionRecord>> {
        json(
            self.http
                .get(self.url(&["sessions"])?)
                .query(&[("open", open_only)]),
        )
    }

    pub fn session(&self, id: &str) -> Result<SessionRecord> {
        json(self.http.get(self.url(&["sessions", id])?))
    }

    pub fn close_session(&self, id: &str) -> Result<()> {
        send(self.http.delete(self.url(&["sessions", id])?))?;
        Ok(())
    }

    /// The session's files within `scopes` (empty means everything).
    pub fn files(&self, session: &str, scopes: &[String]) -> Result<Vec<FileEntry>> {
        json(
            self.http
                .get(self.url(&["sessions", session, "files"])?)
                .query(&[("scope", scopes.join(","))]),
        )
    }

    pub fn read(&self, session: &str, path: &str) -> Result<Option<Vec<u8>>> {
        bytes_or_none(
            self.http
                .get(self.file_url(&["sessions", session, "files"], path)?),
        )
    }

    pub fn write(&self, session: &str, path: &str, content: &[u8]) -> Result<()> {
        send(
            self.http
                .put(self.file_url(&["sessions", session, "files"], path)?)
                .body(content.to_vec()),
        )?;
        Ok(())
    }

    pub fn delete(&self, session: &str, path: &str) -> Result<()> {
        send(
            self.http
                .delete(self.file_url(&["sessions", session, "files"], path)?),
        )?;
        Ok(())
    }

    /// Lands the session. Transport failures are retried with the same key, so a landing
    /// is never applied twice (spec §4.4 receipts).
    pub fn land(&self, session: &str, message: &str, key: &str) -> Result<Receipt> {
        let body = json!({ "message": message, "idempotency_key": key });
        let mut attempt = 0;
        loop {
            attempt += 1;
            match json(
                self.http
                    .post(self.url(&["sessions", session, "land"])?)
                    .json(&body),
            ) {
                Err(ClientError::Http(e)) if attempt < 3 && (e.is_connect() || e.is_timeout()) => {
                    continue;
                }
                other => return other,
            }
        }
    }

    pub fn rebase(&self, session: &str) -> Result<RebaseOutcome> {
        json(self.http.post(self.url(&["sessions", session, "rebase"])?))
    }

    pub fn declare_intent(&self, session: &str, intent: &Intent) -> Result<IntentOutcome> {
        json(
            self.http
                .post(self.url(&["sessions", session, "intent"])?)
                .json(intent),
        )
    }

    /// Every notification for `session`, acknowledged or not.
    pub fn all_notifications(&self, session: &str) -> Result<Vec<Notification>> {
        json(
            self.http
                .get(self.url(&["sessions", session, "notifications"])?)
                .query(&[("all", true)]),
        )
    }

    pub fn notifications(&self, session: &str) -> Result<Vec<Notification>> {
        json(
            self.http
                .get(self.url(&["sessions", session, "notifications"])?),
        )
    }

    pub fn ack_notifications(&self, session: &str, through_seq: u64) -> Result<u64> {
        let v: serde_json::Value = json(
            self.http
                .post(self.url(&["sessions", session, "notifications", "ack"])?)
                .json(&json!({ "through_seq": through_seq })),
        )?;
        Ok(v["acked"].as_u64().unwrap_or(0))
    }

    pub fn conflicts(&self, open_only: bool) -> Result<Vec<ConflictRecord>> {
        json(
            self.http
                .get(self.url(&["conflicts"])?)
                .query(&[("open", open_only)]),
        )
    }

    pub fn conflict(&self, id: &str) -> Result<ConflictRecord> {
        json(self.http.get(self.url(&["conflicts", id])?))
    }

    /// Resolves a conflict with final text per path (`None` deletes).
    pub fn resolve_conflict(
        &self,
        id: &str,
        resolutions: &BTreeMap<String, Option<String>>,
    ) -> Result<RebaseOutcome> {
        json(
            self.http
                .post(self.url(&["conflicts", id, "resolve"])?)
                .json(&json!({ "resolutions": resolutions })),
        )
    }

    pub fn tasks(&self) -> Result<Vec<Task>> {
        json(self.http.get(self.url(&["tasks"])?))
    }

    pub fn create_task(
        &self,
        created_by: &str,
        subject: &str,
        detail: &str,
        scope: &[Scope],
    ) -> Result<Task> {
        json(self.http.post(self.url(&["tasks"])?).json(
            &json!({ "created_by": created_by, "subject": subject, "detail": detail, "scope": scope }),
        ))
    }

    pub fn claim_task(&self, id: &str, principal: &str) -> Result<ClaimOutcome> {
        json(
            self.http
                .post(self.url(&["tasks", id, "claim"])?)
                .json(&json!({ "principal": principal })),
        )
    }

    /// Claims the oldest open task (no race between agents picking the same one).
    pub fn claim_next(&self, principal: &str) -> Result<ClaimOutcome> {
        json(
            self.http
                .post(self.url(&["tasks", "next"])?)
                .json(&json!({ "principal": principal })),
        )
    }

    pub fn release_task(&self, id: &str, principal: &str) -> Result<Task> {
        json(
            self.http
                .post(self.url(&["tasks", id, "release"])?)
                .json(&json!({ "principal": principal })),
        )
    }

    pub fn complete_task(&self, id: &str, principal: &str, summary: &str) -> Result<Task> {
        json(
            self.http
                .post(self.url(&["tasks", id, "complete"])?)
                .json(&json!({ "principal": principal, "summary": summary })),
        )
    }

    pub fn leases(&self) -> Result<Vec<Lease>> {
        json(self.http.get(self.url(&["leases"])?))
    }

    pub fn log_append(&self, author: &str, entry: &NewEntry) -> Result<Entry> {
        let mut body = serde_json::to_value(entry)?;
        body["author"] = json!(author);
        json(self.http.post(self.url(&["log"])?).json(&body))
    }

    /// Entries `reader` has not seen yet, filtered by `topics` (empty means all).
    pub fn log_new(&self, reader: &str, topics: &[String]) -> Result<Vec<Entry>> {
        let topics = topics.join(",");
        json(
            self.http
                .get(self.url(&["log", "new"])?)
                .query(&[("reader", reader), ("topics", topics.as_str())]),
        )
    }

    pub fn log_entries(&self, after: u64, limit: usize) -> Result<Vec<Entry>> {
        json(
            self.http
                .get(self.url(&["log"])?)
                .query(&[("after", after), ("limit", limit as u64)]),
        )
    }

    pub fn log_resolve(&self, seq: u64, by: &str, note: &str) -> Result<Entry> {
        json(
            self.http
                .post(self.url(&["log", &seq.to_string(), "resolve"])?)
                .json(&json!({ "by": by, "note": note })),
        )
    }
}

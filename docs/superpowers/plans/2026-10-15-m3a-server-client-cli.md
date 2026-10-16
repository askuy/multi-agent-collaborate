# M3a: HTTP Server, Client, CLI, Checkouts — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put Trellis behind one HTTP server so any number of agents (in separate processes) can use it, with a Rust client, a `tl` CLI, and sparse checkouts that let agents work on real files (`tl open`, edit, `tl land`).

**Architecture:** `trellis-server` (axum) owns one `Engine` per repository over a shared SQLite metadata store and filesystem blob store; every engine call runs on the blocking pool. All coordination locks are in-process, so **all agents must go through this one server** (the M3b MCP server will be a stdio→HTTP proxy, not a second engine). The API is JSON, except file contents, which travel as raw bytes; notifications also stream over SSE. `trellis-client` is a blocking HTTP client returning the engine's own types, plus the checkout module: a directory bound to a session with a manifest (`.trellis/checkout.json`) of path → content hash; `sync` sends local edits, `update` rebases and refreshes, `resolve` turns the directory's contents into conflict resolutions. `trellis-cli` builds the `tl` binary on top.

**Tech Stack:** axum 0.8, tokio, futures, reqwest 0.12 (blocking, rustls), clap 4.

**Spec:** §3 (architecture), §4.9 (working copies: v1 explicit checkout/sync), §5.1 (HTTP), §5.3 (CLI).

**Security note (v1):** no authentication; the server binds to `127.0.0.1` by default and trusts the `principal` each request names. Do not expose it on a network.

---

## File Structure

```text
Cargo.toml                                 + axum, tokio, futures, reqwest, clap; new crates
crates/trellis-coord/src/task.rs           ClaimOutcome: serde
crates/trellis-coord/src/context.rs        NewEntry: #[serde(default)]
crates/trellis-land/src/records.rs         + FileEntry
crates/trellis-land/src/engine/mod.rs      + session_files, changesets_since
crates/trellis-land/src/engine/intent.rs   IntentOutcome: serde
crates/trellis-server/
  Cargo.toml
  src/lib.rs                               Config, run, start_background
  src/state.rs                             AppState: stores + engine registry
  src/error.rs                             ApiError and status mapping
  src/routes.rs                            every endpoint
  tests/api.rs                             end-to-end through trellis-client
crates/trellis-client/
  Cargo.toml
  src/lib.rs
  src/error.rs                             ClientError
  src/client.rs                            Client: one method per endpoint
  src/checkout.rs                          Checkout, checkout/status/sync/update/resolve/rebind/write_markers
crates/trellis-cli/
  Cargo.toml                               [[bin]] tl
  src/main.rs                              clap commands
  tests/workflow.rs                        two agents collaborating through `tl`
```

---

### Task 1: Engine additions for serving

**Files:**
- Modify: `crates/trellis-coord/src/task.rs`, `crates/trellis-coord/src/context.rs`, `crates/trellis-land/src/records.rs`, `crates/trellis-land/src/lib.rs`, `crates/trellis-land/src/engine/mod.rs`, `crates/trellis-land/src/engine/intent.rs`

- [ ] **Step 1: Serde for the remaining API types**

In `crates/trellis-coord/src/task.rs`, change the derive on `ClaimOutcome` to:

```rust
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum ClaimOutcome {
```

In `crates/trellis-coord/src/context.rs`, add `#[serde(default)]` on the line after the derive of `NewEntry`.

In `crates/trellis-land/src/engine/intent.rs`, change the derive on `IntentOutcome` to:

```rust
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum IntentOutcome {
```

In `crates/trellis-land/src/records.rs`, add after `Notification`:

```rust
/// One file as a session sees it: its base with its own writes on top.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: RepoPath,
    pub blob: BlobRef,
    pub kind: FileKind,
    /// Written in this session and not yet landed.
    pub changed: bool,
}
```

and export it: add `FileEntry` to the `pub use records::{...}` list in `crates/trellis-land/src/lib.rs`.

- [ ] **Step 2: Write the failing tests**

Add to the test module of `crates/trellis-land/src/engine/mod.rs`:

```rust
    #[test]
    fn session_files_overlay_writes_on_the_base_within_scopes() {
        let e = engine();
        seed(&e, "a/x.md", b"1");
        let s = e.open_session("s", None).unwrap();
        e.write(&s.id, &p("a/y.md"), Some(b"2")).unwrap();
        e.write(&s.id, &p("a/x.md"), None).unwrap();
        e.write(&s.id, &p("b/z.md"), Some(b"3")).unwrap();
        let all = e.session_files(&s.id, &[]).unwrap();
        assert_eq!(all.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a/y.md", "b/z.md"]);
        assert!(all.iter().all(|f| f.changed));
        let only_a = e.session_files(&s.id, &[trellis_coord::Scope::parse("a/**").unwrap()]).unwrap();
        assert_eq!(only_a.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a/y.md"]);
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
        assert_eq!(all.iter().map(|(seq, _, _)| *seq).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(all[2].1, e.main().unwrap());
        assert_eq!(e.changesets_since(2).unwrap().len(), 1);
        assert!(e.changesets_since(3).unwrap().is_empty());
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p trellis-land session_files`
Expected: compile error, `no method named 'session_files' found for struct 'Engine'`.

- [ ] **Step 4: Implement**

In `crates/trellis-land/src/engine/mod.rs`, extend the imports: `use trellis_derive::{DeriveCtx, DeriveError, SkipList, TreeIndex, flatten, read_file};`, `use trellis_coord::Scope;` (merge into the existing `trellis_coord` import), and `use crate::records::FileEntry;` (merge into the existing records import). Add to `impl Engine`:

```rust
    /// The files `session` sees (its base plus its own writes), limited to `scopes`
    /// (empty means everything), in path order.
    pub fn session_files(&self, session: &str, scopes: &[Scope]) -> Result<Vec<FileEntry>> {
        let record = self.session(session)?;
        let ctx = self.ctx();
        let root = ctx.get::<TreeIndex>(&record.base)?.root;
        let mut files: BTreeMap<RepoPath, FileEntry> = flatten(&ctx, &root)?
            .into_iter()
            .map(|(path, (blob, kind))| (path.clone(), FileEntry { path, blob, kind, changed: false }))
            .collect();
        for (path, change) in &record.overlay {
            match change {
                FileChange::Upsert { blob, kind } => {
                    files.insert(path.clone(), FileEntry { path: path.clone(), blob: *blob, kind: *kind, changed: true });
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
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p trellis-land && cargo test -p trellis-coord`
Expected: land `test result: ok. 31 passed` (plus the concurrency test); coord `test result: ok. 16 passed`.

- [ ] **Step 6: Commit**

```bash
git add crates/trellis-coord crates/trellis-land
git commit -m "feat(land): add session file listing and main history for serving"
```

---

### Task 2: `trellis-server` and `trellis-client`

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/trellis-server/{Cargo.toml,src/lib.rs,src/state.rs,src/error.rs,src/routes.rs,tests/api.rs}`, `crates/trellis-client/{Cargo.toml,src/lib.rs,src/error.rs,src/client.rs}`

- [ ] **Step 1: Workspace dependencies**

In the root `Cargo.toml` `[workspace.dependencies]`, add:

```toml
trellis-server = { path = "crates/trellis-server" }
trellis-client = { path = "crates/trellis-client" }
axum = "0.8"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "time", "sync", "signal"] }
futures = "0.3"
reqwest = { version = "0.12", default-features = false, features = ["blocking", "json", "rustls-tls"] }
clap = { version = "4", features = ["derive", "env"] }
```

- [ ] **Step 2: The server crate**

`crates/trellis-server/Cargo.toml`:

```toml
[package]
name = "trellis-server"
description = "The Trellis HTTP server: one engine per repository behind a JSON API, with notifications over SSE."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-core = { workspace = true }
trellis-store = { workspace = true }
trellis-coord = { workspace = true }
trellis-land = { workspace = true }
trellis-derive = { workspace = true }
axum = { workspace = true }
tokio = { workspace = true }
futures = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
trellis-client = { workspace = true }
tempfile = { workspace = true }
reqwest = { workspace = true }
```

`crates/trellis-server/src/lib.rs`:

```rust
//! The Trellis HTTP server (spec §5.1). All agents of a repository must share one server:
//! session, task, and lease locks are in-process.

mod error;
mod routes;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;

pub use error::ApiError;
pub use state::{AppState, Config};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub fn router(state: Arc<AppState>) -> axum::Router {
    routes::router(state)
}

/// Serves until Ctrl-C.
pub fn run(config: Config) -> Result<(), BoxError> {
    let state = Arc::new(AppState::open(&config)?);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(config.addr).await?;
        eprintln!("trellis listening on http://{}", listener.local_addr()?);
        axum::serve(listener, router(state))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        Ok(())
    })
}

/// A server on a background thread (tests, embedding). Dropping it signals shutdown.
pub struct Background {
    pub addr: SocketAddr,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Background {
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

pub fn start_background(config: Config) -> Result<Background, BoxError> {
    let state = Arc::new(AppState::open(&config)?);
    let (addr_tx, addr_rx) = std::sync::mpsc::channel::<Result<SocketAddr, String>>();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let addr = config.addr;
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(e) => {
                let _ = addr_tx.send(Err(e.to_string()));
                return;
            }
        };
        runtime.block_on(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(e) => {
                    let _ = addr_tx.send(Err(e.to_string()));
                    return;
                }
            };
            let _ = addr_tx.send(listener.local_addr().map_err(|e| e.to_string()));
            let _ = axum::serve(listener, router(state))
                .with_graceful_shutdown(async {
                    let _ = stop_rx.await;
                })
                .await;
        });
    });
    let addr = addr_rx.recv().map_err(|e| e.to_string())??;
    Ok(Background { addr, shutdown: Some(stop_tx) })
}
```

`crates/trellis-server/src/state.rs`:

```rust
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use trellis_land::{Engine, Env, Policy, SystemEnv};
use trellis_store::{BlobStore, FsBlobStore, MetaStore, SqliteMetaStore};

use crate::BoxError;

pub struct Config {
    pub addr: SocketAddr,
    pub data_dir: PathBuf,
    pub policy: Policy,
}

impl Config {
    pub fn new(addr: SocketAddr, data_dir: impl Into<PathBuf>) -> Self {
        Self { addr, data_dir: data_dir.into(), policy: Policy::default() }
    }
}

/// The shared stores and one engine per repository, created on first use.
pub struct AppState {
    meta: Arc<dyn MetaStore>,
    blobs: Arc<dyn BlobStore>,
    env: Arc<dyn Env>,
    policy: Policy,
    engines: Mutex<HashMap<String, Arc<Engine>>>,
}

impl AppState {
    pub fn open(config: &Config) -> Result<Self, BoxError> {
        std::fs::create_dir_all(&config.data_dir)?;
        Ok(Self {
            meta: Arc::new(SqliteMetaStore::open(config.data_dir.join("meta.db"))?),
            blobs: Arc::new(FsBlobStore::open(config.data_dir.join("objects"))?),
            env: Arc::new(SystemEnv::new()),
            policy: config.policy.clone(),
            engines: Mutex::new(HashMap::new()),
        })
    }

    pub fn engine(&self, repo: &str) -> Arc<Engine> {
        let mut engines = self.engines.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(engines.entry(repo.to_owned()).or_insert_with(|| {
            Arc::new(Engine::new(
                repo,
                Arc::clone(&self.meta),
                Arc::clone(&self.blobs),
                Arc::clone(&self.env),
                self.policy.clone(),
            ))
        }))
    }
}
```

`crates/trellis-server/src/error.rs`:

```rust
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use trellis_coord::CoordError;
use trellis_core::CoreError;
use trellis_derive::DeriveError;
use trellis_land::LandError;

/// An error response: `{"error": <stable code>, "message": <text>}`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into() }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid-input", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({ "error": self.code, "message": self.message }))).into_response()
    }
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        Self::bad_request(e.to_string())
    }
}

impl From<CoordError> for ApiError {
    fn from(e: CoordError) -> Self {
        let message = e.to_string();
        let (status, code) = match e {
            CoordError::InvalidScope(_) => (StatusCode::BAD_REQUEST, "invalid-scope"),
            CoordError::TaskNotFound(_) | CoordError::LeaseNotFound(_) | CoordError::EntryNotFound(_) => {
                (StatusCode::NOT_FOUND, "not-found")
            }
            CoordError::NotTaskOwner { .. } | CoordError::NotLeaseHolder { .. } => (StatusCode::FORBIDDEN, "not-owner"),
            CoordError::Store(_) | CoordError::Encoding(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        Self::new(status, code, message)
    }
}

impl From<LandError> for ApiError {
    fn from(e: LandError) -> Self {
        let message = e.to_string();
        let (status, code) = match e {
            LandError::Coord(inner) => return inner.into(),
            LandError::Core(_) => (StatusCode::BAD_REQUEST, "invalid-input"),
            LandError::Policy { .. } => (StatusCode::BAD_REQUEST, "invalid-policy"),
            LandError::NotInitialized => (StatusCode::NOT_FOUND, "repo-not-initialized"),
            LandError::SessionNotFound(_) | LandError::ConflictNotFound(_) => (StatusCode::NOT_FOUND, "not-found"),
            LandError::Derive(DeriveError::MissingObject(_) | DeriveError::MissingChangeset(_)) => {
                (StatusCode::NOT_FOUND, "not-found")
            }
            LandError::SessionNotOpen(_) => (StatusCode::CONFLICT, "session-not-open"),
            LandError::ConflictNotCurrent(_) => (StatusCode::CONFLICT, "conflict-not-current"),
            LandError::Unresolved { .. } => (StatusCode::CONFLICT, "unresolved"),
            LandError::BaseNotOnMain => (StatusCode::CONFLICT, "base-not-on-main"),
            LandError::Contention(_) => (StatusCode::SERVICE_UNAVAILABLE, "contention"),
            LandError::Store(_) | LandError::Derive(_) | LandError::Encoding(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        Self::new(status, code, message)
    }
}
```

`crates/trellis-server/src/routes.rs`:

```rust
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use trellis_coord::{ClaimOutcome, Entry, Lease, NewEntry, Scope, Task};
use trellis_core::{Changeset, Hash, RepoPath};
use trellis_land::{
    ConflictRecord, Engine, FileEntry, Intent, IntentOutcome, Notification, Receipt, RebaseOutcome,
    Resolutions, SessionRecord,
};

use crate::error::ApiError;
use crate::state::AppState;

type App = State<Arc<AppState>>;
type ApiResult<T> = Result<T, ApiError>;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/repos/{repo}", post(init_repo).get(repo_info))
        .route("/repos/{repo}/main/files/{*path}", get(read_main))
        .route("/repos/{repo}/blobs/{hash}", get(blob))
        .route("/repos/{repo}/changesets", get(changesets))
        .route("/repos/{repo}/sessions", post(open_session).get(list_sessions))
        .route("/repos/{repo}/sessions/{session}", get(get_session).delete(close_session))
        .route("/repos/{repo}/sessions/{session}/files", get(session_files))
        .route(
            "/repos/{repo}/sessions/{session}/files/{*path}",
            get(read_file).put(write_file).delete(delete_file),
        )
        .route("/repos/{repo}/sessions/{session}/land", post(land))
        .route("/repos/{repo}/sessions/{session}/rebase", post(rebase))
        .route("/repos/{repo}/sessions/{session}/intent", post(intent))
        .route("/repos/{repo}/sessions/{session}/notifications", get(notifications))
        .route("/repos/{repo}/sessions/{session}/notifications/ack", post(ack))
        .route("/repos/{repo}/sessions/{session}/events", get(events))
        .route("/repos/{repo}/conflicts", get(conflicts))
        .route("/repos/{repo}/conflicts/{conflict}", get(conflict))
        .route("/repos/{repo}/conflicts/{conflict}/resolve", post(resolve))
        .route("/repos/{repo}/tasks", get(tasks).post(create_task))
        .route("/repos/{repo}/tasks/{task}/claim", post(claim_task))
        .route("/repos/{repo}/tasks/{task}/release", post(release_task))
        .route("/repos/{repo}/tasks/{task}/complete", post(complete_task))
        .route("/repos/{repo}/leases", get(leases))
        .route("/repos/{repo}/log", get(log_entries).post(log_append))
        .route("/repos/{repo}/log/new", get(log_new))
        .route("/repos/{repo}/log/{seq}/resolve", post(log_resolve))
        .with_state(state)
}

fn engine(app: &AppState, repo: &str) -> ApiResult<Arc<Engine>> {
    let valid = !repo.is_empty()
        && repo.len() <= 64
        && repo.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !valid {
        return Err(ApiError::bad_request(format!("invalid repository name {repo:?}")));
    }
    Ok(app.engine(repo))
}

/// Runs a synchronous engine call on the blocking pool.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> ApiResult<T> + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| ApiError::internal(e.to_string()))?
}

fn repo_path(path: &str) -> ApiResult<RepoPath> {
    Ok(RepoPath::parse(path)?)
}

fn scopes(list: Option<&str>) -> ApiResult<Vec<Scope>> {
    list.unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| Scope::parse(s).map_err(ApiError::from))
        .collect()
}

fn bytes(content: Option<Vec<u8>>) -> ApiResult<Response> {
    match content {
        Some(content) => Ok(([(header::CONTENT_TYPE, "application/octet-stream")], content).into_response()),
        None => Err(ApiError::new(StatusCode::NOT_FOUND, "no-such-file", "no such file")),
    }
}

#[derive(Serialize)]
struct RepoInfo {
    repo: String,
    main: Hash,
}

async fn init_repo(State(app): App, Path(repo): Path<String>) -> ApiResult<Json<RepoInfo>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(RepoInfo { main: e.init()?, repo }))).await
}

async fn repo_info(State(app): App, Path(repo): Path<String>) -> ApiResult<Json<RepoInfo>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(RepoInfo { main: e.main()?, repo }))).await
}

async fn read_main(State(app): App, Path((repo, path)): Path<(String, String)>) -> ApiResult<Response> {
    let e = engine(&app, &repo)?;
    blocking(move || bytes(e.read_main(&repo_path(&path)?)?)).await
}

async fn blob(State(app): App, Path((repo, hash)): Path<(String, String)>) -> ApiResult<Response> {
    let e = engine(&app, &repo)?;
    let hash: Hash = hash.parse()?;
    blocking(move || bytes(Some(e.read_blob(&hash)?))).await
}

#[derive(Deserialize)]
struct AfterSeq {
    #[serde(default)]
    after_seq: u64,
}

#[derive(Serialize)]
struct ChangesetEntry {
    seq: u64,
    id: Hash,
    changeset: Changeset,
}

async fn changesets(State(app): App, Path(repo): Path<String>, Query(q): Query<AfterSeq>) -> ApiResult<Json<Vec<ChangesetEntry>>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        let list = e.changesets_since(q.after_seq)?;
        Ok(Json(list.into_iter().map(|(seq, id, changeset)| ChangesetEntry { seq, id, changeset }).collect()))
    })
    .await
}

#[derive(Deserialize)]
struct OpenSession {
    principal: String,
    task: Option<String>,
}

async fn open_session(State(app): App, Path(repo): Path<String>, Json(body): Json<OpenSession>) -> ApiResult<Json<SessionRecord>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.open_session(&body.principal, body.task.as_deref())?))).await
}

#[derive(Deserialize)]
struct OpenOnly {
    #[serde(default)]
    open: bool,
}

async fn list_sessions(State(app): App, Path(repo): Path<String>, Query(q): Query<OpenOnly>) -> ApiResult<Json<Vec<SessionRecord>>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.sessions(q.open)?))).await
}

async fn get_session(State(app): App, Path((repo, session)): Path<(String, String)>) -> ApiResult<Json<SessionRecord>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.session(&session)?))).await
}

async fn close_session(State(app): App, Path((repo, session)): Path<(String, String)>) -> ApiResult<StatusCode> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        e.close_session(&session)?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

#[derive(Deserialize)]
struct ScopeQuery {
    scope: Option<String>,
}

async fn session_files(
    State(app): App,
    Path((repo, session)): Path<(String, String)>,
    Query(q): Query<ScopeQuery>,
) -> ApiResult<Json<Vec<FileEntry>>> {
    let e = engine(&app, &repo)?;
    let scopes = scopes(q.scope.as_deref())?;
    blocking(move || Ok(Json(e.session_files(&session, &scopes)?))).await
}

async fn read_file(State(app): App, Path((repo, session, path)): Path<(String, String, String)>) -> ApiResult<Response> {
    let e = engine(&app, &repo)?;
    blocking(move || bytes(e.read(&session, &repo_path(&path)?)?)).await
}

async fn write_file(
    State(app): App,
    Path((repo, session, path)): Path<(String, String, String)>,
    body: Bytes,
) -> ApiResult<StatusCode> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        e.write(&session, &repo_path(&path)?, Some(&body))?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

async fn delete_file(State(app): App, Path((repo, session, path)): Path<(String, String, String)>) -> ApiResult<StatusCode> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        e.write(&session, &repo_path(&path)?, None)?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

#[derive(Deserialize)]
struct LandBody {
    message: String,
    idempotency_key: String,
}

async fn land(State(app): App, Path((repo, session)): Path<(String, String)>, Json(body): Json<LandBody>) -> ApiResult<Json<Receipt>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.land(&session, &body.message, &body.idempotency_key)?))).await
}

async fn rebase(State(app): App, Path((repo, session)): Path<(String, String)>) -> ApiResult<Json<RebaseOutcome>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.rebase(&session)?))).await
}

async fn intent(State(app): App, Path((repo, session)): Path<(String, String)>, Json(body): Json<Intent>) -> ApiResult<Json<IntentOutcome>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.declare_intent(&session, &body)?))).await
}

async fn notifications(State(app): App, Path((repo, session)): Path<(String, String)>) -> ApiResult<Json<Vec<Notification>>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.notifications(&session)?))).await
}

#[derive(Deserialize)]
struct AckBody {
    through_seq: u64,
}

async fn ack(State(app): App, Path((repo, session)): Path<(String, String)>, Json(body): Json<AckBody>) -> ApiResult<Json<serde_json::Value>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(serde_json::json!({ "acked": e.ack_notifications(&session, body.through_seq)? })))).await
}

/// Streams new notifications for a session as SSE `notification` events, each carrying a
/// JSON array of notifications.
async fn events(
    State(app): App,
    Path((repo, session)): Path<(String, String)>,
) -> ApiResult<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>> {
    let e = engine(&app, &repo)?;
    let check = (Arc::clone(&e), session.clone());
    blocking(move || Ok(check.0.session(&check.1).map(|_| ())?)).await?;
    let stream = futures::stream::unfold((e, session, 0u64), |(engine, session, seen)| async move {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let (e, s) = (Arc::clone(&engine), session.clone());
            let notes = match tokio::task::spawn_blocking(move || e.notifications(&s)).await {
                Ok(Ok(notes)) => notes,
                _ => return None,
            };
            let fresh: Vec<Notification> = notes.into_iter().filter(|n| n.seq > seen).collect();
            if let Some(last) = fresh.last().map(|n| n.seq) {
                let event = Event::default().event("notification").json_data(&fresh).ok()?;
                return Some((Ok(event), (engine, session, last)));
            }
        }
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(1))))
}

async fn conflicts(State(app): App, Path(repo): Path<String>, Query(q): Query<OpenOnly>) -> ApiResult<Json<Vec<ConflictRecord>>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.conflicts(q.open)?))).await
}

async fn conflict(State(app): App, Path((repo, id)): Path<(String, String)>) -> ApiResult<Json<ConflictRecord>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.conflict(&id)?))).await
}

#[derive(Deserialize)]
struct ResolveBody {
    /// Path → final text (`null` deletes the file).
    resolutions: BTreeMap<String, Option<String>>,
}

async fn resolve(State(app): App, Path((repo, id)): Path<(String, String)>, Json(body): Json<ResolveBody>) -> ApiResult<Json<RebaseOutcome>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        let mut resolutions = Resolutions::new();
        for (path, text) in body.resolutions {
            resolutions.insert(repo_path(&path)?, text.map(String::into_bytes));
        }
        Ok(Json(e.resolve_conflict(&id, &resolutions)?))
    })
    .await
}

async fn tasks(State(app): App, Path(repo): Path<String>) -> ApiResult<Json<Vec<Task>>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().tasks()?))).await
}

#[derive(Deserialize)]
struct CreateTask {
    created_by: String,
    subject: String,
    #[serde(default)]
    detail: String,
    #[serde(default)]
    scope: Vec<Scope>,
}

async fn create_task(State(app): App, Path(repo): Path<String>, Json(b): Json<CreateTask>) -> ApiResult<Json<Task>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().create_task(&b.created_by, &b.subject, &b.detail, b.scope)?))).await
}

#[derive(Deserialize)]
struct PrincipalBody {
    principal: String,
    #[serde(default)]
    summary: String,
}

async fn claim_task(State(app): App, Path((repo, task)): Path<(String, String)>, Json(b): Json<PrincipalBody>) -> ApiResult<Json<ClaimOutcome>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().claim_task(&task, &b.principal)?))).await
}

async fn release_task(State(app): App, Path((repo, task)): Path<(String, String)>, Json(b): Json<PrincipalBody>) -> ApiResult<Json<Task>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().release_task(&task, &b.principal)?))).await
}

async fn complete_task(State(app): App, Path((repo, task)): Path<(String, String)>, Json(b): Json<PrincipalBody>) -> ApiResult<Json<Task>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().complete_task(&task, &b.principal, &b.summary)?))).await
}

async fn leases(State(app): App, Path(repo): Path<String>) -> ApiResult<Json<Vec<Lease>>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().active_leases()?))).await
}

#[derive(Deserialize)]
struct LogQuery {
    #[serde(default)]
    after: u64,
    limit: Option<usize>,
}

async fn log_entries(State(app): App, Path(repo): Path<String>, Query(q): Query<LogQuery>) -> ApiResult<Json<Vec<Entry>>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().entries(q.after, q.limit.unwrap_or(usize::MAX))?))).await
}

#[derive(Deserialize)]
struct AppendBody {
    author: String,
    #[serde(flatten)]
    entry: NewEntry,
}

async fn log_append(State(app): App, Path(repo): Path<String>, Json(b): Json<AppendBody>) -> ApiResult<Json<Entry>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().append(&b.author, b.entry)?))).await
}

#[derive(Deserialize)]
struct NewQuery {
    reader: String,
    topics: Option<String>,
}

async fn log_new(State(app): App, Path(repo): Path<String>, Query(q): Query<NewQuery>) -> ApiResult<Json<Vec<Entry>>> {
    let e = engine(&app, &repo)?;
    let topics: Vec<String> =
        q.topics.unwrap_or_default().split(',').filter(|t| !t.is_empty()).map(str::to_owned).collect();
    blocking(move || Ok(Json(e.coord().read_new(&q.reader, &topics)?))).await
}

#[derive(Deserialize)]
struct ResolveEntry {
    by: String,
    #[serde(default)]
    note: String,
}

async fn log_resolve(State(app): App, Path((repo, seq)): Path<(String, u64)>, Json(b): Json<ResolveEntry>) -> ApiResult<Json<Entry>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().resolve_entry(seq, &b.by, &b.note)?))).await
}
```

- [ ] **Step 3: The client crate**

`crates/trellis-client/Cargo.toml`:

```toml
[package]
name = "trellis-client"
description = "A blocking Rust client for the Trellis HTTP server, and directory checkouts bound to sessions."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-core = { workspace = true }
trellis-coord = { workspace = true }
trellis-land = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
```

`crates/trellis-client/src/lib.rs`:

```rust
//! A blocking client for the Trellis server (spec §5.1) and directory checkouts (§4.9).

pub mod checkout;
mod client;
mod error;

pub use checkout::{Changes, Checkout};
pub use client::{ChangesetEntry, Client};
pub use error::{ClientError, Result};
```

`crates/trellis-client/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid server url {0}")]
    InvalidServer(String),
    #[error("{status} {code}: {message}")]
    Api { status: u16, code: String, message: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

impl ClientError {
    /// The server's stable error code, for API errors.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Api { code, .. } => Some(code),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;
```

`crates/trellis-client/src/client.rs`:

```rust
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
    ConflictRecord, FileEntry, Intent, IntentOutcome, Notification, Receipt, RebaseOutcome, SessionRecord,
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
        Err(ClientError::Api { status: 404, code, .. }) if code == "no-such-file" => Ok(None),
        Err(e) => Err(e),
    }
}

impl Client {
    pub fn new(server: &str, repo: &str) -> Result<Self> {
        let base = Url::parse(server).map_err(|e| ClientError::InvalidServer(format!("{server}: {e}")))?;
        let http = Http::builder().timeout(Duration::from_secs(120)).build()?;
        Ok(Self { base, repo: repo.to_owned(), http })
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
        Ok(send(self.http.get(self.url(&["blobs", &hash.to_hex()])?))?.bytes()?.to_vec())
    }

    pub fn changesets_since(&self, after_seq: u64) -> Result<Vec<ChangesetEntry>> {
        json(self.http.get(self.url(&["changesets"])?).query(&[("after_seq", after_seq)]))
    }

    pub fn open_session(&self, principal: &str, task: Option<&str>) -> Result<SessionRecord> {
        json(self.http.post(self.url(&["sessions"])?).json(&json!({ "principal": principal, "task": task })))
    }

    pub fn sessions(&self, open_only: bool) -> Result<Vec<SessionRecord>> {
        json(self.http.get(self.url(&["sessions"])?).query(&[("open", open_only)]))
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
        json(self.http.get(self.url(&["sessions", session, "files"])?).query(&[("scope", scopes.join(","))]))
    }

    pub fn read(&self, session: &str, path: &str) -> Result<Option<Vec<u8>>> {
        bytes_or_none(self.http.get(self.file_url(&["sessions", session, "files"], path)?))
    }

    pub fn write(&self, session: &str, path: &str, content: &[u8]) -> Result<()> {
        send(self.http.put(self.file_url(&["sessions", session, "files"], path)?).body(content.to_vec()))?;
        Ok(())
    }

    pub fn delete(&self, session: &str, path: &str) -> Result<()> {
        send(self.http.delete(self.file_url(&["sessions", session, "files"], path)?))?;
        Ok(())
    }

    /// Lands the session. Transport failures are retried with the same key, so a landing
    /// is never applied twice (spec §4.4 receipts).
    pub fn land(&self, session: &str, message: &str, key: &str) -> Result<Receipt> {
        let body = json!({ "message": message, "idempotency_key": key });
        let mut attempt = 0;
        loop {
            attempt += 1;
            match json(self.http.post(self.url(&["sessions", session, "land"])?).json(&body)) {
                Err(ClientError::Http(e)) if attempt < 3 && (e.is_connect() || e.is_timeout()) => continue,
                other => return other,
            }
        }
    }

    pub fn rebase(&self, session: &str) -> Result<RebaseOutcome> {
        json(self.http.post(self.url(&["sessions", session, "rebase"])?))
    }

    pub fn declare_intent(&self, session: &str, intent: &Intent) -> Result<IntentOutcome> {
        json(self.http.post(self.url(&["sessions", session, "intent"])?).json(intent))
    }

    pub fn notifications(&self, session: &str) -> Result<Vec<Notification>> {
        json(self.http.get(self.url(&["sessions", session, "notifications"])?))
    }

    pub fn ack_notifications(&self, session: &str, through_seq: u64) -> Result<u64> {
        let v: serde_json::Value = json(
            self.http.post(self.url(&["sessions", session, "notifications", "ack"])?).json(&json!({ "through_seq": through_seq })),
        )?;
        Ok(v["acked"].as_u64().unwrap_or(0))
    }

    pub fn conflicts(&self, open_only: bool) -> Result<Vec<ConflictRecord>> {
        json(self.http.get(self.url(&["conflicts"])?).query(&[("open", open_only)]))
    }

    pub fn conflict(&self, id: &str) -> Result<ConflictRecord> {
        json(self.http.get(self.url(&["conflicts", id])?))
    }

    /// Resolves a conflict with final text per path (`None` deletes).
    pub fn resolve_conflict(&self, id: &str, resolutions: &BTreeMap<String, Option<String>>) -> Result<RebaseOutcome> {
        json(self.http.post(self.url(&["conflicts", id, "resolve"])?).json(&json!({ "resolutions": resolutions })))
    }

    pub fn tasks(&self) -> Result<Vec<Task>> {
        json(self.http.get(self.url(&["tasks"])?))
    }

    pub fn create_task(&self, created_by: &str, subject: &str, detail: &str, scope: &[Scope]) -> Result<Task> {
        json(self.http.post(self.url(&["tasks"])?).json(
            &json!({ "created_by": created_by, "subject": subject, "detail": detail, "scope": scope }),
        ))
    }

    pub fn claim_task(&self, id: &str, principal: &str) -> Result<ClaimOutcome> {
        json(self.http.post(self.url(&["tasks", id, "claim"])?).json(&json!({ "principal": principal })))
    }

    pub fn release_task(&self, id: &str, principal: &str) -> Result<Task> {
        json(self.http.post(self.url(&["tasks", id, "release"])?).json(&json!({ "principal": principal })))
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
        json(self.http.get(self.url(&["log", "new"])?).query(&[("reader", reader), ("topics", topics.as_str())]))
    }

    pub fn log_entries(&self, after: u64, limit: usize) -> Result<Vec<Entry>> {
        json(self.http.get(self.url(&["log"])?).query(&[("after", after), ("limit", limit as u64)]))
    }

    pub fn log_resolve(&self, seq: u64, by: &str, note: &str) -> Result<Entry> {
        json(self.http.post(self.url(&["log", &seq.to_string(), "resolve"])?).json(&json!({ "by": by, "note": note })))
    }
}
```

Create an empty `crates/trellis-client/src/checkout.rs` for now containing only:

```rust
//! Directory checkouts bound to a session (Task 4).

/// Placeholder until Task 4.
pub struct Checkout;

/// Placeholder until Task 4.
pub struct Changes;
```

- [ ] **Step 4: Write the end-to-end API test**

`crates/trellis-server/tests/api.rs`:

```rust
//! The server and the client together, over real HTTP.

use std::collections::BTreeMap;

use trellis_client::{Client, ClientError};
use trellis_coord::{ClaimOutcome, EntryKind, NewEntry, Scope};
use trellis_land::{Intent, IntentOutcome, Outcome, RebaseOutcome};
use trellis_server::{Background, Config, start_background};

fn server() -> (Background, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let bg = start_background(Config::new("127.0.0.1:0".parse().unwrap(), dir.path().join("data"))).unwrap();
    (bg, dir)
}

fn landed(outcome: &Outcome) -> trellis_core::Hash {
    match outcome {
        Outcome::Landed { changeset, .. } => *changeset,
        other => panic!("expected a landing, got {other:?}"),
    }
}

#[test]
fn files_sessions_and_landing_round_trip() {
    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    let root = c.init().unwrap();
    assert_eq!(c.main().unwrap(), root);
    let s = c.open_session("alice", Some("T-1")).unwrap();
    let odd = "docs/my notes/章节 1.md";
    c.write(&s.id, odd, b"hello").unwrap();
    c.write(&s.id, "README.md", b"# demo\n").unwrap();
    assert_eq!(c.read(&s.id, odd).unwrap().as_deref(), Some(&b"hello"[..]));
    assert_eq!(c.read(&s.id, "missing.md").unwrap(), None);
    let files = c.files(&s.id, &[]).unwrap();
    assert_eq!(files.len(), 2);
    let only_docs = c.files(&s.id, &["docs/**".into()]).unwrap();
    assert_eq!(only_docs.len(), 1);
    assert_eq!(c.blob(&only_docs[0].blob.hash).unwrap(), b"hello");

    let receipt = c.land(&s.id, "first", "k1").unwrap();
    let cs = landed(&receipt.outcome);
    assert_eq!(c.land(&s.id, "first", "k1").unwrap(), receipt);
    assert_eq!(c.main().unwrap(), cs);
    assert_eq!(c.read_main(odd).unwrap().as_deref(), Some(&b"hello"[..]));
    let history = c.changesets_since(0).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].changeset.message, "first");
    assert_eq!(history[0].changeset.provenance.task.as_deref(), Some("T-1"));
    assert_eq!(c.sessions(true).unwrap().len(), 0);
}

#[test]
fn errors_map_to_status_codes() {
    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    let not_init = c.main().unwrap_err();
    assert_eq!(not_init.code(), Some("repo-not-initialized"));
    c.init().unwrap();
    assert_eq!(c.session("nope").unwrap_err().code(), Some("not-found"));
    let s = c.open_session("a", None).unwrap();
    assert!(matches!(c.write(&s.id, "a\\b", b"x"), Err(ClientError::Api { status: 400, .. })));
    c.close_session(&s.id).unwrap();
    assert_eq!(c.write(&s.id, "a.txt", b"x").unwrap_err().code(), Some("session-not-open"));
    let bad = Client::new(&bg.url(), "bad name").unwrap();
    assert!(matches!(bad.init(), Err(ClientError::Api { status: 400, .. })));
}

#[test]
fn coordination_endpoints_work_end_to_end() {
    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();

    let task = c.create_task("planner", "chapter 1", "write it", &[Scope::parse("book/ch1/**").unwrap()]).unwrap();
    assert!(matches!(c.claim_task(&task.id, "alice").unwrap(), ClaimOutcome::Claimed(_)));
    assert!(matches!(c.claim_task(&task.id, "bob").unwrap(), ClaimOutcome::Taken { .. }));
    assert_eq!(c.complete_task(&task.id, "bob", "x").unwrap_err().code(), Some("not-owner"));

    let alice = c.open_session("alice", Some(&task.id)).unwrap();
    let intent = Intent {
        description: "chapter 1".into(),
        writes: vec![Scope::parse("book/ch1/**").unwrap()],
        exclusive: true,
        ttl_ms: 60_000,
        ..Default::default()
    };
    assert!(matches!(c.declare_intent(&alice.id, &intent).unwrap(), IntentOutcome::Admitted { .. }));
    assert_eq!(c.leases().unwrap().len(), 1);

    let watcher = c.open_session("watcher", None).unwrap();
    let watch = Intent { reads: vec![Scope::parse("book/**").unwrap()], ..Default::default() };
    c.declare_intent(&watcher.id, &watch).unwrap();
    c.write(&alice.id, "book/ch1/s1.md", b"scene").unwrap();
    landed(&c.land(&alice.id, "scene 1", "k").unwrap().outcome);
    c.complete_task(&task.id, "alice", "wrote scene 1").unwrap();
    let notes = c.notifications(&watcher.id).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(c.ack_notifications(&watcher.id, notes[0].seq).unwrap(), 1);
    assert!(c.notifications(&watcher.id).unwrap().is_empty());
    assert!(c.leases().unwrap().is_empty());

    let fail = NewEntry {
        kind: Some(EntryKind::Fail),
        topics: vec!["ch1".into()],
        body: "timeline broken".into(),
        ..Default::default()
    };
    let entry = c.log_append("checker", &fail).unwrap();
    assert_eq!(c.log_new("alice", &[]).unwrap().len(), 1);
    assert!(c.log_new("alice", &[]).unwrap().is_empty());
    c.log_resolve(entry.seq, "alice", "fixed").unwrap();
    assert_eq!(c.log_entries(0, 10).unwrap()[0].resolution.as_deref(), Some("alice: fixed"));
}

#[test]
fn conflicts_are_resolved_over_http() {
    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();
    let seed = c.open_session("seed", None).unwrap();
    c.write(&seed.id, "doc.md", b"one\ntwo\nthree\n").unwrap();
    landed(&c.land(&seed.id, "seed", "k").unwrap().outcome);
    let a = c.open_session("a", None).unwrap();
    let b = c.open_session("b", None).unwrap();
    c.write(&a.id, "doc.md", b"one\nTWO-A\nthree\n").unwrap();
    c.write(&b.id, "doc.md", b"one\nTWO-B\nthree\n").unwrap();
    landed(&c.land(&a.id, "a", "k").unwrap().outcome);
    let Outcome::Conflicted { conflict } = c.land(&b.id, "b", "k").unwrap().outcome else {
        panic!("expected a conflict")
    };
    assert_eq!(c.conflicts(true).unwrap().len(), 1);
    assert!(c.conflict(&conflict).unwrap().paths[0].detail.contains("<<<<<<<"));
    let mut resolutions = BTreeMap::new();
    resolutions.insert("doc.md".to_owned(), Some("one\nTWO-A+B\nthree\n".to_owned()));
    assert!(matches!(c.resolve_conflict(&conflict, &resolutions).unwrap(), RebaseOutcome::Rebased { .. }));
    landed(&c.land(&b.id, "b", "k2").unwrap().outcome);
    assert_eq!(c.read_main("doc.md").unwrap().as_deref(), Some(&b"one\nTWO-A+B\nthree\n"[..]));
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p trellis-server`
Expected: `tests/api.rs` `test result: ok. 4 passed`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/trellis-server crates/trellis-client
git commit -m "feat(server): add the HTTP server and a blocking client"
```

---

### Task 3: Notifications over SSE

**Files:**
- Modify: `crates/trellis-server/tests/api.rs`

- [ ] **Step 1: Write the test**

Append to `crates/trellis-server/tests/api.rs`:

```rust
#[test]
fn notifications_stream_over_server_sent_events() {
    use std::io::Read;

    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();
    let watcher = c.open_session("watcher", None).unwrap();
    c.declare_intent(&watcher.id, &Intent { reads: vec![Scope::All], ..Default::default() }).unwrap();

    let url = format!("{}/repos/demo/sessions/{}/events", bg.url(), watcher.id);
    let mut stream = reqwest::blocking::Client::new().get(url).send().unwrap();
    assert!(stream.status().is_success());

    let writer = c.open_session("writer", None).unwrap();
    c.write(&writer.id, "a.md", b"x").unwrap();
    landed(&c.land(&writer.id, "write a", "k").unwrap().outcome);

    let mut seen = String::new();
    let mut buf = [0u8; 1024];
    while !seen.contains("event: notification") {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0, "stream ended before a notification arrived");
        seen.push_str(&String::from_utf8_lossy(&buf[..n]));
    }
    assert!(seen.contains("write a"), "{seen}");
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p trellis-server --test api notifications_stream`
Expected: `test result: ok. 1 passed` (the endpoint was written in Task 2; this test exercises it).

- [ ] **Step 3: Commit**

```bash
git add crates/trellis-server
git commit -m "test(server): cover notifications over server-sent events"
```

---

### Task 4: Checkouts

**Files:**
- Replace: `crates/trellis-client/src/checkout.rs`
- Modify: `crates/trellis-server/tests/api.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/trellis-server/tests/api.rs`:

```rust
#[test]
fn checkouts_sync_update_and_resolve() {
    use trellis_client::checkout;

    let (bg, dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();
    let a_dir = dir.path().join("alice");
    let b_dir = dir.path().join("bob");
    let all = vec!["**".to_owned()];

    let a = c.open_session("alice", None).unwrap();
    checkout::checkout(&c, &a.id, "alice", &all, &a_dir).unwrap();
    std::fs::create_dir_all(a_dir.join("book")).unwrap();
    std::fs::write(a_dir.join("book/ch1.md"), "one\ntwo\nthree\n").unwrap();
    let (_, changes) = checkout::status(&a_dir).unwrap();
    assert_eq!(changes.written, vec!["book/ch1.md".to_owned()]);
    let synced = checkout::sync(&a_dir).unwrap();
    assert_eq!(synced.written.len(), 1);
    assert!(checkout::status(&a_dir).unwrap().1.is_empty());
    landed(&c.land(&a.id, "ch1", "k").unwrap().outcome);

    // Bob checks out after Alice landed and sees her file.
    let b = c.open_session("bob", None).unwrap();
    checkout::checkout(&c, &b.id, "bob", &all, &b_dir).unwrap();
    assert_eq!(std::fs::read_to_string(b_dir.join("book/ch1.md")).unwrap(), "one\ntwo\nthree\n");

    // Alice continues in a new session; both edit different lines.
    let a2 = c.open_session("alice", None).unwrap();
    checkout::rebind(&a_dir, &a2.id).unwrap();
    std::fs::write(a_dir.join("book/ch1.md"), "ONE\ntwo\nthree\n").unwrap();
    std::fs::write(b_dir.join("book/ch1.md"), "one\ntwo\nTHREE\n").unwrap();
    checkout::sync(&a_dir).unwrap();
    landed(&c.land(&a2.id, "a", "k").unwrap().outcome);

    // Bob was notified (his checkout watches everything) and updates: merge, then refresh.
    assert_eq!(c.notifications(&b.id).unwrap().len(), 1);
    assert!(matches!(checkout::update(&b_dir).unwrap(), RebaseOutcome::Rebased { .. }));
    assert_eq!(std::fs::read_to_string(b_dir.join("book/ch1.md")).unwrap(), "ONE\ntwo\nTHREE\n");

    // A real conflict, resolved by editing the file with markers.
    std::fs::write(b_dir.join("book/ch1.md"), "ONE\nTWO-B\nTHREE\n").unwrap();
    let a3 = c.open_session("alice", None).unwrap();
    checkout::rebind(&a_dir, &a3.id).unwrap();
    std::fs::write(a_dir.join("book/ch1.md"), "ONE\nTWO-A\nthree\n").unwrap();
    checkout::sync(&a_dir).unwrap();
    landed(&c.land(&a3.id, "a3", "k").unwrap().outcome);
    checkout::sync(&b_dir).unwrap();
    let Outcome::Conflicted { conflict } = c.land(&b.id, "b", "k").unwrap().outcome else {
        panic!("expected a conflict")
    };
    let written = checkout::write_markers(&b_dir, &conflict).unwrap();
    assert_eq!(written, vec!["book/ch1.md".to_owned()]);
    assert!(std::fs::read_to_string(b_dir.join("book/ch1.md")).unwrap().contains("<<<<<<<"));
    std::fs::write(b_dir.join("book/ch1.md"), "ONE\nTWO-A+B\nthree\n").unwrap();
    assert!(matches!(checkout::resolve(&b_dir, &conflict).unwrap(), RebaseOutcome::Rebased { .. }));
    landed(&c.land(&b.id, "b", "k2").unwrap().outcome);
    assert_eq!(c.read_main("book/ch1.md").unwrap().as_deref(), Some(&b"ONE\nTWO-A+B\nthree\n"[..]));

    // Deleting a file in the directory deletes it in the session.
    let b2 = c.open_session("bob", None).unwrap();
    checkout::rebind(&b_dir, &b2.id).unwrap();
    std::fs::remove_file(b_dir.join("book/ch1.md")).unwrap();
    assert_eq!(checkout::sync(&b_dir).unwrap().deleted, vec!["book/ch1.md".to_owned()]);
    landed(&c.land(&b2.id, "delete", "k").unwrap().outcome);
    assert_eq!(c.read_main("book/ch1.md").unwrap(), None);
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p trellis-server --test api checkouts`
Expected: compile error, `cannot find function 'checkout' in module 'checkout'`.

- [ ] **Step 3: Implement checkouts**

Replace `crates/trellis-client/src/checkout.rs` with:

```rust
//! Directory checkouts bound to a session (spec §4.9, v1: explicit checkout and sync).
//!
//! A checkout is a directory plus `.trellis/checkout.json`, which records the server,
//! repository, session, scopes, and the content hash of every file as last synced. `sync`
//! sends local edits to the session; `update` rebases the session and refreshes the files.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use trellis_coord::Scope;
use trellis_core::{Hash, RepoPath};
use trellis_land::{Intent, RebaseOutcome};

use crate::client::Client;
use crate::error::{ClientError, Result};

const META_DIR: &str = ".trellis";
const MANIFEST: &str = "checkout.json";

/// A directory bound to a session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkout {
    pub server: String,
    pub repo: String,
    pub session: String,
    pub principal: String,
    pub scopes: Vec<String>,
    /// Path → content hash as last synced with the session.
    pub files: BTreeMap<String, Hash>,
}

/// Local edits not yet sent to the session.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changes {
    pub written: Vec<String>,
    pub deleted: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.written.is_empty() && self.deleted.is_empty()
    }
}

fn parse_scopes(scopes: &[String]) -> Result<Vec<Scope>> {
    scopes.iter().map(|s| Scope::parse(s).map_err(|e| ClientError::Invalid(e.to_string()))).collect()
}

impl Checkout {
    pub fn load(dir: &Path) -> Result<Self> {
        let bytes = fs::read(dir.join(META_DIR).join(MANIFEST))
            .map_err(|e| ClientError::Invalid(format!("{} is not a checkout: {e}", dir.display())))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn save(&self, dir: &Path) -> Result<()> {
        let meta = dir.join(META_DIR);
        fs::create_dir_all(&meta)?;
        fs::write(meta.join(MANIFEST), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn client(&self) -> Result<Client> {
        Client::new(&self.server, &self.repo)
    }

    fn in_scope(&self, path: &RepoPath) -> Result<bool> {
        let scopes = parse_scopes(&self.scopes)?;
        Ok(scopes.is_empty() || scopes.iter().any(|s| s.covers(path)))
    }
}

/// Watches the checkout's scopes from `session`, so landings there notify it.
fn watch(client: &Client, session: &str, scopes: &[String], dir: &Path) -> Result<()> {
    let intent = Intent {
        description: format!("checkout at {}", dir.display()),
        reads: parse_scopes(scopes)?,
        ..Default::default()
    };
    client.declare_intent(session, &intent)?;
    Ok(())
}

/// Makes the directory match the session's view within the checkout's scopes.
fn materialize(client: &Client, checkout: &mut Checkout, dir: &Path) -> Result<()> {
    let view = client.files(&checkout.session, &checkout.scopes)?;
    let mut next = BTreeMap::new();
    for entry in view {
        let rel = entry.path.as_str().to_owned();
        let target = dir.join(&rel);
        if checkout.files.get(&rel) != Some(&entry.blob.hash) || !target.exists() {
            let content = client.blob(&entry.blob.hash)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, content)?;
        }
        next.insert(rel, entry.blob.hash);
    }
    for old in checkout.files.keys() {
        if !next.contains_key(old) {
            let _ = fs::remove_file(dir.join(old));
        }
    }
    checkout.files = next;
    Ok(())
}

/// Materializes `session`'s view of `scopes` (empty means everything) into `dir`.
pub fn checkout(client: &Client, session: &str, principal: &str, scopes: &[String], dir: &Path) -> Result<Checkout> {
    let scopes = if scopes.is_empty() { vec!["**".to_owned()] } else { scopes.to_vec() };
    watch(client, session, &scopes, dir)?;
    fs::create_dir_all(dir)?;
    let mut co = Checkout {
        server: client.server().to_owned(),
        repo: client.repo().to_owned(),
        session: session.to_owned(),
        principal: principal.to_owned(),
        scopes,
        files: BTreeMap::new(),
    };
    materialize(client, &mut co, dir)?;
    co.save(dir)?;
    Ok(co)
}

/// Binds an existing checkout to another session (typically a new one after landing) and
/// refreshes the files to its view. Refuses if there are unsynced local edits.
pub fn rebind(dir: &Path, session: &str) -> Result<Checkout> {
    let (mut co, changes) = status(dir)?;
    if !changes.is_empty() {
        return Err(ClientError::Invalid("the checkout has unsynced edits; sync or discard them first".into()));
    }
    let client = co.client()?;
    watch(&client, session, &co.scopes, dir)?;
    co.session = session.to_owned();
    materialize(&client, &mut co, dir)?;
    co.save(dir)?;
    Ok(co)
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" || (dir == root && name == META_DIR) {
            continue;
        }
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            walk(root, &path, out)?;
        } else if kind.is_file() {
            let rel = path.strip_prefix(root).map_err(|e| ClientError::Invalid(e.to_string()))?;
            let rel: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            out.insert(rel.join("/"), path);
        }
    }
    Ok(())
}

/// Local edits relative to the last sync, within the checkout's scopes.
pub fn status(dir: &Path) -> Result<(Checkout, Changes)> {
    let co = Checkout::load(dir)?;
    let mut local = BTreeMap::new();
    walk(dir, dir, &mut local)?;
    let mut changes = Changes::default();
    for (rel, file) in &local {
        let Ok(path) = RepoPath::parse(rel) else { continue };
        if !co.in_scope(&path)? {
            continue;
        }
        if co.files.get(rel) != Some(&Hash::of(&fs::read(file)?)) {
            changes.written.push(rel.clone());
        }
    }
    for rel in co.files.keys() {
        if !local.contains_key(rel) {
            changes.deleted.push(rel.clone());
        }
    }
    Ok((co, changes))
}

/// Sends local edits to the session.
pub fn sync(dir: &Path) -> Result<Changes> {
    let (mut co, changes) = status(dir)?;
    let client = co.client()?;
    for rel in &changes.written {
        let content = fs::read(dir.join(rel))?;
        client.write(&co.session, rel, &content)?;
        co.files.insert(rel.clone(), Hash::of(&content));
    }
    for rel in &changes.deleted {
        client.delete(&co.session, rel)?;
        co.files.remove(rel);
    }
    co.save(dir)?;
    Ok(changes)
}

/// Syncs, rebases the session onto the latest `main`, and refreshes the directory.
pub fn update(dir: &Path) -> Result<RebaseOutcome> {
    sync(dir)?;
    let mut co = Checkout::load(dir)?;
    let client = co.client()?;
    let outcome = client.rebase(&co.session)?;
    if matches!(outcome, RebaseOutcome::Rebased { .. }) {
        materialize(&client, &mut co, dir)?;
        co.save(dir)?;
    }
    Ok(outcome)
}

/// Writes each text conflict's merge, with conflict markers, into its file so it can be
/// edited into the resolution. Returns the paths written.
pub fn write_markers(dir: &Path, conflict: &str) -> Result<Vec<String>> {
    let co = Checkout::load(dir)?;
    let record = co.client()?.conflict(conflict)?;
    let mut written = Vec::new();
    for path in &record.paths {
        if path.reason == "TextOverlap" {
            fs::write(dir.join(path.path.as_str()), &path.detail)?;
            written.push(path.path.as_str().to_owned());
        }
    }
    Ok(written)
}

/// Resolves a conflict with the directory's current contents of every conflicting path (a
/// missing file resolves to a delete), then refreshes the directory.
pub fn resolve(dir: &Path, conflict: &str) -> Result<RebaseOutcome> {
    sync(dir)?;
    let mut co = Checkout::load(dir)?;
    let client = co.client()?;
    let record = client.conflict(conflict)?;
    let mut resolutions = BTreeMap::new();
    for path in &record.paths {
        let file = dir.join(path.path.as_str());
        let content = if file.exists() { Some(fs::read_to_string(&file)?) } else { None };
        resolutions.insert(path.path.as_str().to_owned(), content);
    }
    let outcome = client.resolve_conflict(conflict, &resolutions)?;
    if matches!(outcome, RebaseOutcome::Rebased { .. }) {
        materialize(&client, &mut co, dir)?;
        co.save(dir)?;
    }
    Ok(outcome)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p trellis-server`
Expected: `tests/api.rs` `test result: ok. 6 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-client crates/trellis-server
git commit -m "feat(client): add directory checkouts with sync, update, and conflict resolution"
```

---

### Task 5: The `tl` CLI

**Files:**
- Create: `crates/trellis-cli/Cargo.toml`, `crates/trellis-cli/src/main.rs`, `crates/trellis-cli/tests/workflow.rs`

- [ ] **Step 1: Create the crate**

`crates/trellis-cli/Cargo.toml`:

```toml
[package]
name = "trellis-cli"
description = "tl: the Trellis command line."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[[bin]]
name = "tl"
path = "src/main.rs"

[dependencies]
trellis-client = { workspace = true }
trellis-server = { workspace = true }
trellis-coord = { workspace = true }
trellis-land = { workspace = true }
clap = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

`crates/trellis-cli/src/main.rs`:

```rust
//! `tl`: the Trellis command line (spec §5.3). Every command prints JSON on stdout.

use std::error::Error;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;
use trellis_client::checkout::{self, Checkout};
use trellis_client::Client;
use trellis_coord::{EntryKind, NewEntry, Scope};
use trellis_land::{Outcome, Policy};

#[derive(Parser)]
#[command(name = "tl", version, about = "Trellis: collaboration infrastructure for agent swarms")]
struct Cli {
    /// Server URL.
    #[arg(long, global = true, env = "TRELLIS_SERVER", default_value = "http://127.0.0.1:7420")]
    server: String,
    /// Repository name.
    #[arg(long, global = true, env = "TRELLIS_REPO", default_value = "main")]
    repo: String,
    /// Who is acting (an agent or person name).
    #[arg(long = "as", global = true, env = "TRELLIS_PRINCIPAL", default_value = "human")]
    principal: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server.
    Serve {
        #[arg(long, default_value = "127.0.0.1:7420")]
        addr: SocketAddr,
        #[arg(long, default_value = ".trellis-data")]
        data: PathBuf,
        /// A trellis.policy.yaml file.
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    /// Create the repository if needed and print main.
    Init,
    /// Open a session and check it out into DIR.
    Open {
        dir: PathBuf,
        #[arg(long)]
        task: Option<String>,
        /// Limit the checkout (a path, `dir/**`, or `**`); repeatable.
        #[arg(long = "scope")]
        scopes: Vec<String>,
    },
    /// Open a new session for an existing checkout (after landing).
    Next {
        dir: PathBuf,
        #[arg(long)]
        task: Option<String>,
    },
    /// Show unsynced edits and pending notifications.
    Status { dir: PathBuf },
    /// Send local edits to the session.
    Sync { dir: PathBuf },
    /// Sync, rebase onto the latest main, and refresh the files.
    Update { dir: PathBuf },
    /// Sync and land. Exit code 2 means conflicted, 3 means rejected.
    Land {
        dir: PathBuf,
        #[arg(short, long)]
        message: String,
        /// Idempotency key; reuse it to retry safely.
        #[arg(long)]
        key: Option<String>,
    },
    /// List notifications; `--ack` acknowledges them.
    Notes {
        dir: PathBuf,
        #[arg(long)]
        ack: bool,
    },
    /// List open conflicts.
    Conflicts,
    /// Inspect or resolve one conflict.
    Conflict {
        #[command(subcommand)]
        action: ConflictAction,
    },
    /// The task queue.
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// The shared context log.
    Log {
        #[command(subcommand)]
        action: LogAction,
    },
    /// Active leases.
    Leases,
}

#[derive(Subcommand)]
enum ConflictAction {
    Show { id: String },
    /// Write each text conflict's merge, with markers, into the checkout's files.
    Markers { dir: PathBuf, id: String },
    /// Resolve with the checkout's current file contents.
    Resolve { dir: PathBuf, id: String },
}

#[derive(Subcommand)]
enum TaskAction {
    List,
    Create {
        subject: String,
        #[arg(long, default_value = "")]
        detail: String,
        #[arg(long = "scope")]
        scopes: Vec<String>,
    },
    Claim { id: String },
    Release { id: String },
    Complete {
        id: String,
        #[arg(long, default_value = "")]
        summary: String,
    },
}

#[derive(Subcommand)]
enum LogAction {
    Add {
        body: String,
        /// fact, fail, summary, decision, or note.
        #[arg(long, default_value = "note")]
        kind: String,
        #[arg(long = "topic")]
        topics: Vec<String>,
        #[arg(long = "scope")]
        scopes: Vec<String>,
    },
    /// Entries this principal has not seen yet.
    Read {
        #[arg(long = "topic")]
        topics: Vec<String>,
    },
    Resolve {
        seq: u64,
        #[arg(long, default_value = "")]
        note: String,
    },
}

type CliResult = Result<ExitCode, Box<dyn Error>>;

fn print<T: Serialize>(value: &T) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn scopes(list: &[String]) -> Result<Vec<Scope>, Box<dyn Error>> {
    Ok(list.iter().map(|s| Scope::parse(s)).collect::<Result<Vec<_>, _>>()?)
}

fn generated_key() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("tl-{nanos:x}")
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("tl: {e}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> CliResult {
    let client = || Client::new(&cli.server, &cli.repo);
    match cli.command {
        Command::Serve { addr, data, policy } => {
            let policy = match policy {
                Some(path) => Policy::from_yaml(&fs::read_to_string(path)?)?,
                None => Policy::default(),
            };
            let mut config = trellis_server::Config::new(addr, data);
            config.policy = policy;
            trellis_server::run(config).map_err(|e| e.to_string())?;
        }
        Command::Init => print(&json!({ "repo": cli.repo, "main": client()?.init()? }))?,
        Command::Open { dir, task, scopes } => {
            let c = client()?;
            let session = c.open_session(&cli.principal, task.as_deref())?;
            let co = checkout::checkout(&c, &session.id, &cli.principal, &scopes, &dir)?;
            print(&json!({ "session": session.id, "base": session.base, "files": co.files.len() }))?;
        }
        Command::Next { dir, task } => {
            let co = Checkout::load(&dir)?;
            let session = co.client()?.open_session(&co.principal, task.as_deref())?;
            let co = checkout::rebind(&dir, &session.id)?;
            print(&json!({ "session": session.id, "base": session.base, "files": co.files.len() }))?;
        }
        Command::Status { dir } => {
            let (co, changes) = checkout::status(&dir)?;
            let notes = co.client()?.notifications(&co.session)?;
            print(&json!({ "session": co.session, "changes": changes, "notifications": notes }))?;
        }
        Command::Sync { dir } => print(&checkout::sync(&dir)?)?,
        Command::Update { dir } => print(&checkout::update(&dir)?)?,
        Command::Land { dir, message, key } => {
            checkout::sync(&dir)?;
            let co = Checkout::load(&dir)?;
            let receipt = co.client()?.land(&co.session, &message, &key.unwrap_or_else(generated_key))?;
            print(&receipt)?;
            return Ok(match receipt.outcome {
                Outcome::Landed { .. } | Outcome::Unchanged { .. } => ExitCode::SUCCESS,
                Outcome::Conflicted { .. } => ExitCode::from(2),
                Outcome::Rejected { .. } => ExitCode::from(3),
            });
        }
        Command::Notes { dir, ack } => {
            let co = Checkout::load(&dir)?;
            let c = co.client()?;
            let notes = c.notifications(&co.session)?;
            if ack {
                if let Some(last) = notes.last() {
                    c.ack_notifications(&co.session, last.seq)?;
                }
            }
            print(&notes)?;
        }
        Command::Conflicts => print(&client()?.conflicts(true)?)?,
        Command::Conflict { action } => match action {
            ConflictAction::Show { id } => print(&client()?.conflict(&id)?)?,
            ConflictAction::Markers { dir, id } => print(&checkout::write_markers(&dir, &id)?)?,
            ConflictAction::Resolve { dir, id } => print(&checkout::resolve(&dir, &id)?)?,
        },
        Command::Task { action } => {
            let c = client()?;
            match action {
                TaskAction::List => print(&c.tasks()?)?,
                TaskAction::Create { subject, detail, scopes: list } => {
                    print(&c.create_task(&cli.principal, &subject, &detail, &scopes(&list)?)?)?
                }
                TaskAction::Claim { id } => print(&c.claim_task(&id, &cli.principal)?)?,
                TaskAction::Release { id } => print(&c.release_task(&id, &cli.principal)?)?,
                TaskAction::Complete { id, summary } => print(&c.complete_task(&id, &cli.principal, &summary)?)?,
            }
        }
        Command::Log { action } => {
            let c = client()?;
            match action {
                LogAction::Add { body, kind, topics, scopes: list } => {
                    let kind: EntryKind = serde_json::from_value(json!(kind))?;
                    let entry = NewEntry { kind: Some(kind), topics, scope: scopes(&list)?, body, evidence: Vec::new() };
                    print(&c.log_append(&cli.principal, &entry)?)?;
                }
                LogAction::Read { topics } => print(&c.log_new(&cli.principal, &topics)?)?,
                LogAction::Resolve { seq, note } => print(&c.log_resolve(seq, &cli.principal, &note)?)?,
            }
        }
        Command::Leases => print(&client()?.leases()?)?,
    }
    Ok(ExitCode::SUCCESS)
}
```

- [ ] **Step 2: Write the workflow test**

`crates/trellis-cli/tests/workflow.rs`:

```rust
//! Two agents collaborating through `tl` against a real server.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn tl(server: &str, principal: &str, args: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_tl"))
        .args(["--server", server, "--repo", "demo", "--as", principal])
        .args(args)
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert!(
        matches!(code, 0 | 2 | 3),
        "tl {args:?} failed ({code}): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (code, serde_json::from_slice(&out.stdout).unwrap_or(Value::Null))
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn two_agents_collaborate_through_checkouts() {
    let tmp = tempfile::tempdir().unwrap();
    let bg = trellis_server::start_background(trellis_server::Config::new(
        "127.0.0.1:0".parse().unwrap(),
        tmp.path().join("data"),
    ))
    .unwrap();
    let url = bg.url();
    let (a, b, c) = (tmp.path().join("alice"), tmp.path().join("bob"), tmp.path().join("carol"));

    tl(&url, "planner", &["init"]);
    let (_, task) = tl(&url, "planner", &["task", "create", "chapter 1", "--scope", "book/**"]);
    let task_id = task["id"].as_str().unwrap().to_owned();
    let (_, claim) = tl(&url, "alice", &["task", "claim", &task_id]);
    assert_eq!(claim["outcome"], "claimed");

    tl(&url, "alice", &["open", path(&a), "--task", &task_id]);
    std::fs::create_dir_all(a.join("book")).unwrap();
    std::fs::write(a.join("book/ch1.md"), "one\ntwo\nthree\n").unwrap();
    let (code, receipt) = tl(&url, "alice", &["land", path(&a), "-m", "chapter 1"]);
    assert_eq!((code, receipt["outcome"]["outcome"].as_str()), (0, Some("landed")));

    tl(&url, "bob", &["open", path(&b)]);
    assert_eq!(std::fs::read_to_string(b.join("book/ch1.md")).unwrap(), "one\ntwo\nthree\n");

    tl(&url, "alice", &["next", path(&a)]);
    std::fs::write(a.join("book/ch1.md"), "ONE\ntwo\nthree\n").unwrap();
    std::fs::write(b.join("book/ch1.md"), "one\ntwo\nTHREE\n").unwrap();
    tl(&url, "alice", &["land", path(&a), "-m", "alice edits"]);
    let (_, status) = tl(&url, "bob", &["status", path(&b)]);
    assert_eq!(status["notifications"].as_array().unwrap().len(), 1);
    assert_eq!(status["changes"]["written"][0], "book/ch1.md");
    let (code, receipt) = tl(&url, "bob", &["land", path(&b), "-m", "bob edits"]);
    assert_eq!((code, receipt["outcome"]["route"].as_str()), (0, Some("composed")));

    // A conflict, resolved through the checkout.
    tl(&url, "alice", &["next", path(&a)]);
    tl(&url, "bob", &["next", path(&b)]);
    std::fs::write(a.join("book/ch1.md"), "ONE\nTWO-A\nTHREE\n").unwrap();
    std::fs::write(b.join("book/ch1.md"), "ONE\nTWO-B\nTHREE\n").unwrap();
    tl(&url, "alice", &["land", path(&a), "-m", "alice line 2"]);
    let (code, receipt) = tl(&url, "bob", &["land", path(&b), "-m", "bob line 2"]);
    assert_eq!(code, 2);
    let conflict = receipt["outcome"]["conflict"].as_str().unwrap().to_owned();
    tl(&url, "bob", &["conflict", "markers", path(&b), &conflict]);
    std::fs::write(b.join("book/ch1.md"), "ONE\nTWO-A+B\nTHREE\n").unwrap();
    tl(&url, "bob", &["conflict", "resolve", path(&b), &conflict]);
    let (code, _) = tl(&url, "bob", &["land", path(&b), "-m", "bob resolved"]);
    assert_eq!(code, 0);

    tl(&url, "carol", &["open", path(&c)]);
    assert_eq!(std::fs::read_to_string(c.join("book/ch1.md")).unwrap(), "ONE\nTWO-A+B\nTHREE\n");

    tl(&url, "alice", &["log", "add", "chapter 1 drafted", "--kind", "summary", "--topic", "ch1"]);
    let (_, entries) = tl(&url, "carol", &["log", "read", "--topic", "ch1"]);
    assert_eq!(entries.as_array().unwrap().len(), 1);
    let (_, done) = tl(&url, "alice", &["task", "complete", &task_id, "--summary", "drafted"]);
    assert_eq!(done["status"]["state"], "done");
}
```

- [ ] **Step 3: Run it**

Run: `cargo test -p trellis-cli`
Expected: `test result: ok. 1 passed`.

- [ ] **Step 4: Commit**

```bash
git add Cargo.lock crates/trellis-cli
git commit -m "feat(cli): add the tl command line"
```

---

### Task 6: Final verification and docs

- [ ] **Step 1: Run every check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: clippy exits 0; all suites `ok`.

- [ ] **Step 2: README quick start**

In `README.md`, update the status line to say the HTTP server, client, and `tl` CLI with checkouts are built (MCP and the Git bridge are next), and add after the Interfaces section:

````markdown
## Quick Start

```bash
cargo install --path crates/trellis-cli          # installs `tl`
tl serve &                                         # http://127.0.0.1:7420, data in .trellis-data
export TRELLIS_REPO=demo
tl init
tl --as alice open ./alice                         # session + checkout
echo "hello" > ./alice/notes.md
tl --as alice land ./alice -m "first note"         # exit 2 = conflict, 3 = rejected
tl --as bob open ./bob && cat ./bob/notes.md
```
````

- [ ] **Step 3: Commit**

```bash
git add README.md docs/superpowers/plans/2026-10-15-m3a-server-client-cli.md
git commit -m "docs: add a quick start for the server, client, and CLI"
```

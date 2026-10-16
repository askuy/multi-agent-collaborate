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
    ConflictRecord, Engine, FileEntry, Intent, IntentOutcome, Notification, RebaseOutcome, Receipt,
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
        .route(
            "/repos/{repo}/sessions",
            post(open_session).get(list_sessions),
        )
        .route(
            "/repos/{repo}/sessions/{session}",
            get(get_session).delete(close_session),
        )
        .route("/repos/{repo}/sessions/{session}/files", get(session_files))
        .route(
            "/repos/{repo}/sessions/{session}/files/{*path}",
            get(read_file).put(write_file).delete(delete_file),
        )
        .route("/repos/{repo}/sessions/{session}/land", post(land))
        .route("/repos/{repo}/sessions/{session}/rebase", post(rebase))
        .route("/repos/{repo}/sessions/{session}/intent", post(intent))
        .route(
            "/repos/{repo}/sessions/{session}/notifications",
            get(notifications),
        )
        .route(
            "/repos/{repo}/sessions/{session}/notifications/ack",
            post(ack),
        )
        .route("/repos/{repo}/sessions/{session}/events", get(events))
        .route("/repos/{repo}/conflicts", get(conflicts))
        .route("/repos/{repo}/conflicts/{conflict}", get(conflict))
        .route("/repos/{repo}/conflicts/{conflict}/resolve", post(resolve))
        .route("/repos/{repo}/tasks", get(tasks).post(create_task))
        .route("/repos/{repo}/tasks/next", post(claim_next))
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
        && repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !valid {
        return Err(ApiError::bad_request(format!(
            "invalid repository name {repo:?}"
        )));
    }
    Ok(app.engine(repo))
}

/// Runs a synchronous engine call on the blocking pool.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
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
        Some(content) => Ok((
            [(header::CONTENT_TYPE, "application/octet-stream")],
            content,
        )
            .into_response()),
        None => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "no-such-file",
            "no such file",
        )),
    }
}

#[derive(Serialize)]
struct RepoInfo {
    repo: String,
    main: Hash,
}

async fn init_repo(State(app): App, Path(repo): Path<String>) -> ApiResult<Json<RepoInfo>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        Ok(Json(RepoInfo {
            main: e.init()?,
            repo,
        }))
    })
    .await
}

async fn repo_info(State(app): App, Path(repo): Path<String>) -> ApiResult<Json<RepoInfo>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        Ok(Json(RepoInfo {
            main: e.main()?,
            repo,
        }))
    })
    .await
}

async fn read_main(
    State(app): App,
    Path((repo, path)): Path<(String, String)>,
) -> ApiResult<Response> {
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

async fn changesets(
    State(app): App,
    Path(repo): Path<String>,
    Query(q): Query<AfterSeq>,
) -> ApiResult<Json<Vec<ChangesetEntry>>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        let list = e.changesets_since(q.after_seq)?;
        Ok(Json(
            list.into_iter()
                .map(|(seq, id, changeset)| ChangesetEntry { seq, id, changeset })
                .collect(),
        ))
    })
    .await
}

#[derive(Deserialize)]
struct OpenSession {
    principal: String,
    task: Option<String>,
}

async fn open_session(
    State(app): App,
    Path(repo): Path<String>,
    Json(body): Json<OpenSession>,
) -> ApiResult<Json<SessionRecord>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.open_session(&body.principal, body.task.as_deref())?))).await
}

#[derive(Deserialize)]
struct OpenOnly {
    #[serde(default)]
    open: bool,
}

async fn list_sessions(
    State(app): App,
    Path(repo): Path<String>,
    Query(q): Query<OpenOnly>,
) -> ApiResult<Json<Vec<SessionRecord>>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.sessions(q.open)?))).await
}

async fn get_session(
    State(app): App,
    Path((repo, session)): Path<(String, String)>,
) -> ApiResult<Json<SessionRecord>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.session(&session)?))).await
}

async fn close_session(
    State(app): App,
    Path((repo, session)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
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

async fn read_file(
    State(app): App,
    Path((repo, session, path)): Path<(String, String, String)>,
) -> ApiResult<Response> {
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

async fn delete_file(
    State(app): App,
    Path((repo, session, path)): Path<(String, String, String)>,
) -> ApiResult<StatusCode> {
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

async fn land(
    State(app): App,
    Path((repo, session)): Path<(String, String)>,
    Json(body): Json<LandBody>,
) -> ApiResult<Json<Receipt>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        Ok(Json(e.land(
            &session,
            &body.message,
            &body.idempotency_key,
        )?))
    })
    .await
}

async fn rebase(
    State(app): App,
    Path((repo, session)): Path<(String, String)>,
) -> ApiResult<Json<RebaseOutcome>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.rebase(&session)?))).await
}

async fn intent(
    State(app): App,
    Path((repo, session)): Path<(String, String)>,
    Json(body): Json<Intent>,
) -> ApiResult<Json<IntentOutcome>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.declare_intent(&session, &body)?))).await
}

#[derive(Deserialize)]
struct AllQuery {
    /// Include acknowledged notifications.
    #[serde(default)]
    all: bool,
}

async fn notifications(
    State(app): App,
    Path((repo, session)): Path<(String, String)>,
    Query(q): Query<AllQuery>,
) -> ApiResult<Json<Vec<Notification>>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        let notes = if q.all {
            e.all_notifications(&session)?
        } else {
            e.notifications(&session)?
        };
        Ok(Json(notes))
    })
    .await
}

#[derive(Deserialize)]
struct AckBody {
    through_seq: u64,
}

async fn ack(
    State(app): App,
    Path((repo, session)): Path<(String, String)>,
    Json(body): Json<AckBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        Ok(Json(
            serde_json::json!({ "acked": e.ack_notifications(&session, body.through_seq)? }),
        ))
    })
    .await
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
    let stream =
        futures::stream::unfold((e, session, 0u64), |(engine, session, seen)| async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let (e, s) = (Arc::clone(&engine), session.clone());
                let notes = match tokio::task::spawn_blocking(move || e.notifications(&s)).await {
                    Ok(Ok(notes)) => notes,
                    _ => return None,
                };
                let fresh: Vec<Notification> = notes.into_iter().filter(|n| n.seq > seen).collect();
                if let Some(last) = fresh.last().map(|n| n.seq) {
                    let event = Event::default()
                        .event("notification")
                        .json_data(&fresh)
                        .ok()?;
                    return Some((Ok(event), (engine, session, last)));
                }
            }
        });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(1))))
}

async fn conflicts(
    State(app): App,
    Path(repo): Path<String>,
    Query(q): Query<OpenOnly>,
) -> ApiResult<Json<Vec<ConflictRecord>>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.conflicts(q.open)?))).await
}

async fn conflict(
    State(app): App,
    Path((repo, id)): Path<(String, String)>,
) -> ApiResult<Json<ConflictRecord>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.conflict(&id)?))).await
}

#[derive(Deserialize)]
struct ResolveBody {
    /// Path → final text (`null` deletes the file).
    resolutions: BTreeMap<String, Option<String>>,
}

async fn resolve(
    State(app): App,
    Path((repo, id)): Path<(String, String)>,
    Json(body): Json<ResolveBody>,
) -> ApiResult<Json<RebaseOutcome>> {
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

async fn create_task(
    State(app): App,
    Path(repo): Path<String>,
    Json(b): Json<CreateTask>,
) -> ApiResult<Json<Task>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        Ok(Json(e.coord().create_task(
            &b.created_by,
            &b.subject,
            &b.detail,
            b.scope,
        )?))
    })
    .await
}

#[derive(Deserialize)]
struct PrincipalBody {
    principal: String,
    #[serde(default)]
    summary: String,
}

async fn claim_task(
    State(app): App,
    Path((repo, task)): Path<(String, String)>,
    Json(b): Json<PrincipalBody>,
) -> ApiResult<Json<ClaimOutcome>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().claim_task(&task, &b.principal)?))).await
}

async fn claim_next(
    State(app): App,
    Path(repo): Path<String>,
    Json(b): Json<PrincipalBody>,
) -> ApiResult<Json<ClaimOutcome>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().claim_next(&b.principal)?))).await
}

async fn release_task(
    State(app): App,
    Path((repo, task)): Path<(String, String)>,
    Json(b): Json<PrincipalBody>,
) -> ApiResult<Json<Task>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().release_task(&task, &b.principal)?))).await
}

async fn complete_task(
    State(app): App,
    Path((repo, task)): Path<(String, String)>,
    Json(b): Json<PrincipalBody>,
) -> ApiResult<Json<Task>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        Ok(Json(e.coord().complete_task(
            &task,
            &b.principal,
            &b.summary,
        )?))
    })
    .await
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

async fn log_entries(
    State(app): App,
    Path(repo): Path<String>,
    Query(q): Query<LogQuery>,
) -> ApiResult<Json<Vec<Entry>>> {
    let e = engine(&app, &repo)?;
    blocking(move || {
        Ok(Json(
            e.coord().entries(q.after, q.limit.unwrap_or(usize::MAX))?,
        ))
    })
    .await
}

#[derive(Deserialize)]
struct AppendBody {
    author: String,
    #[serde(flatten)]
    entry: NewEntry,
}

async fn log_append(
    State(app): App,
    Path(repo): Path<String>,
    Json(b): Json<AppendBody>,
) -> ApiResult<Json<Entry>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().append(&b.author, b.entry)?))).await
}

#[derive(Deserialize)]
struct NewQuery {
    reader: String,
    topics: Option<String>,
}

async fn log_new(
    State(app): App,
    Path(repo): Path<String>,
    Query(q): Query<NewQuery>,
) -> ApiResult<Json<Vec<Entry>>> {
    let e = engine(&app, &repo)?;
    let topics: Vec<String> = q
        .topics
        .unwrap_or_default()
        .split(',')
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect();
    blocking(move || Ok(Json(e.coord().read_new(&q.reader, &topics)?))).await
}

#[derive(Deserialize)]
struct ResolveEntry {
    by: String,
    #[serde(default)]
    note: String,
}

async fn log_resolve(
    State(app): App,
    Path((repo, seq)): Path<(String, u64)>,
    Json(b): Json<ResolveEntry>,
) -> ApiResult<Json<Entry>> {
    let e = engine(&app, &repo)?;
    blocking(move || Ok(Json(e.coord().resolve_entry(seq, &b.by, &b.note)?))).await
}

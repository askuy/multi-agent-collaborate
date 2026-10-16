use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use trellis_client::{Client, ClientError};
use trellis_coord::{ClaimOutcome, EntryKind, NewEntry, Scope};
use trellis_land::{Intent, IntentOutcome, Outcome, RebaseOutcome};

/// How often a waiting exclusive intent retries.
const LOCK_POLL_MS: u64 = 200;

/// Returned by `initialize`: how to collaborate through Trellis.
pub const INSTRUCTIONS: &str = "\
Trellis is a shared, versioned workspace for many agents working at once.

Workflow (batch: every round trip costs you a model turn, so do in one call what you can):
1. task_next claims the oldest open task and opens a session bound to it. Or: task_claim a specific task, then
   session_open with its id, or session_open alone for untracked work. Writes stay private until you land.
2. read the files you depend on in one call (`paths`; reads are tracked), then write them in one call (`files`).
   Use list to see files.
3. land with a short message and a `summary` to complete the task. Outcomes: landed (done; task_next for more), unchanged,
   conflicted (call conflict_show, then conflict_resolve with the final text of each path, then land again),
   rejected (a strict path moved or someone holds a lease: rebase, re-check, and land again).
4. Tool results may end with '[trellis] ... change(s) landed on files you read or watch'. Decide whether your plan
   still holds; if not, rebase and re-read those files. Nobody is blocked: you repair what is affected.

Coordinate cheaply: log_append facts you discover and FAILs you find in others' work (log_read shows only what is new);
use intent with exclusive=true only for small scopes you must own (it makes others' landings there fail), and prefer
watching scopes (reads) to locking them. Scopes are a path, `dir/**`, or `**`.";

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn strings(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": description })
}

/// Every tool, for `tools/list`.
pub fn definitions() -> Vec<Value> {
    vec![
        tool(
            "session_open",
            "Open your working session (private until you land). Optionally bind it to a claimed task.",
            json!({ "task": string("Task id") }),
            &[],
        ),
        tool(
            "session_close",
            "Abandon your session and discard its unlanded writes.",
            json!({}),
            &[],
        ),
        tool(
            "read",
            "Read files as your session sees them; pass `paths` to read several in one call. Reads are tracked so you are told when someone changes them.",
            json!({ "path": string("Repository-relative path"), "paths": strings("Several paths at once") }),
            &[],
        ),
        tool(
            "write",
            "Write text files in your session: `path` and `content`, or `files` mapping several paths to their full new content.",
            json!({
                "path": string("Repository-relative path"),
                "content": string("Full new content"),
                "files": { "type": "object", "additionalProperties": { "type": "string" }, "description": "Path to full new content" },
            }),
            &[],
        ),
        tool(
            "append",
            "Append text (a newline is added if missing) to the end of a file in your session, creating it if needed, without sending the whole file. For append-style files such as changelogs.",
            json!({ "path": string("Repository-relative path"), "text": string("Text to add at the end") }),
            &["path", "text"],
        ),
        tool(
            "delete",
            "Delete a file in your session.",
            json!({ "path": string("Repository-relative path") }),
            &["path"],
        ),
        tool(
            "list",
            "List files your session sees; `(changed)` marks your unlanded writes.",
            json!({ "scope": string("A path, `dir/**`, or `**` (default)") }),
            &[],
        ),
        tool(
            "land",
            "Land your session onto main. Read the outcome: landed, unchanged, conflicted, or rejected. With `summary`, a landed session also completes its task.",
            json!({ "message": string("What this change does"), "summary": string("Task summary: completes the session's task once landed") }),
            &["message"],
        ),
        tool(
            "rebase",
            "Move your session onto the latest main, merging your writes.",
            json!({}),
            &[],
        ),
        tool(
            "intent",
            "Declare what you will read and write. Reads are watched for changes; with exclusive=true your writes are leased so others' landings there are rejected. Use exclusive sparingly and for small scopes.",
            json!({
                "description": string("What you are doing"),
                "reads": strings("Scopes you depend on"),
                "writes": strings("Scopes you will write"),
                "exclusive": { "type": "boolean", "description": "Lease the write scopes" },
                "ttl_seconds": { "type": "integer", "description": "Lease duration (default 600)" },
                "wait_seconds": { "type": "integer", "description": "With exclusive: wait up to this long for others' leases to go away instead of returning the conflict (default 0)" },
                "rebase": { "type": "boolean", "description": "Once admitted, rebase onto the latest main so your reads see it" },
            }),
            &["description"],
        ),
        tool(
            "notifications",
            "Changes landed on files you read or watch. ack=true marks them seen.",
            json!({ "ack": { "type": "boolean" } }),
            &[],
        ),
        tool(
            "conflicts",
            "Open conflicts in the repository.",
            json!({}),
            &[],
        ),
        tool(
            "conflict_show",
            "A conflict's paths, with merge markers for text files.",
            json!({ "id": string("Conflict id") }),
            &["id"],
        ),
        tool(
            "conflict_resolve",
            "Resolve a conflict with the final text of every conflicting path (null deletes it). Then land again.",
            json!({ "id": string("Conflict id"), "resolutions": { "type": "object", "additionalProperties": { "type": ["string", "null"] } } }),
            &["id", "resolutions"],
        ),
        tool(
            "task_next",
            "Claim the oldest open task and open a session bound to it, in one call. Says so when no task is open.",
            json!({}),
            &[],
        ),
        tool("task_list", "The shared task queue.", json!({}), &[]),
        tool(
            "task_create",
            "Add a task to the queue.",
            json!({ "subject": string("Short title"), "detail": string("Intended output, inputs, and interfaces"), "scope": strings("Scopes the task covers") }),
            &["subject"],
        ),
        tool(
            "task_claim",
            "Claim a task. The first claim wins; you may hold one unfinished claim.",
            json!({ "id": string("Task id") }),
            &["id"],
        ),
        tool(
            "task_release",
            "Give a claimed task back.",
            json!({ "id": string("Task id") }),
            &["id"],
        ),
        tool(
            "task_complete",
            "Mark your claimed task done.",
            json!({ "id": string("Task id"), "summary": string("What was done") }),
            &["id"],
        ),
        tool(
            "log_append",
            "Add to the shared log: fact (a finding with evidence), fail (a defect or dead end; stays open until resolved), summary, decision, or note.",
            json!({ "kind": string("fact | fail | summary | decision | note"), "body": string("The entry"), "topics": strings("Topics for filtering"), "scope": strings("Scopes it concerns") }),
            &["kind", "body"],
        ),
        tool(
            "log_read",
            "Log entries you have not seen yet (optionally only these topics).",
            json!({ "topics": strings("Topics") }),
            &[],
        ),
        tool(
            "log_resolve",
            "Resolve a fail entry.",
            json!({ "seq": { "type": "integer" }, "note": string("How it was resolved") }),
            &["seq"],
        ),
        tool(
            "leases",
            "Active leases: who owns what, why, and until when.",
            json!({}),
            &[],
        ),
    ]
}

/// One agent: its identity, its current session, and the notifications already shown.
pub struct Agent {
    client: Client,
    principal: String,
    session: Option<String>,
    /// The task the open session is bound to.
    task: Option<String>,
    shown_seq: u64,
    next_key: u64,
}

type ToolResult = Result<Value, String>;

fn arg(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing string argument `{key}`"))
}

fn opt(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn scopes(args: &Value, key: &str) -> Result<Vec<Scope>, String> {
    list(args, key)
        .iter()
        .map(|s| Scope::parse(s).map_err(|e| e.to_string()))
        .collect()
}

fn failed(e: ClientError) -> String {
    e.to_string()
}

fn to_json<T: serde::Serialize>(value: &T) -> ToolResult {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

impl Agent {
    pub fn new(client: Client, principal: String) -> Self {
        Self {
            client,
            principal,
            session: None,
            task: None,
            shown_seq: 0,
            next_key: 0,
        }
    }

    /// Runs a tool and wraps the outcome as an MCP tool result.
    pub fn call(&mut self, name: &str, args: &Value) -> Value {
        let (mut text, is_error) = match self.dispatch(name, args) {
            Ok(Value::String(text)) => (text, false),
            Ok(value) => (
                serde_json::to_string_pretty(&value).unwrap_or_default(),
                false,
            ),
            Err(message) => (message, true),
        };
        if let Some(notes) = self.pending_notes() {
            text.push_str(&notes);
        }
        json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
    }

    fn session(&self) -> Result<String, String> {
        self.session
            .clone()
            .ok_or_else(|| "no open session: call session_open first".to_owned())
    }

    /// Notifications not yet shown to this agent, as a trailer for the next tool result.
    fn pending_notes(&mut self) -> Option<String> {
        let session = self.session.clone()?;
        let notes = self.client.notifications(&session).ok()?;
        let fresh: Vec<_> = notes
            .into_iter()
            .filter(|n| n.seq > self.shown_seq)
            .collect();
        self.shown_seq = fresh.last()?.seq;
        let lines: Vec<String> = fresh
            .iter()
            .map(|n| {
                let paths: Vec<&str> = n.paths.iter().map(|p| p.as_str()).collect();
                format!(
                    "- {} landed \"{}\" (seq {}), touching {}",
                    n.author,
                    n.message,
                    n.seq,
                    paths.join(", ")
                )
            })
            .collect();
        Some(format!(
            "\n\n[trellis] {} change(s) landed on files you read or watch. Decide whether your plan still holds; if not, rebase and re-read them:\n{}",
            fresh.len(),
            lines.join("\n")
        ))
    }

    fn dispatch(&mut self, name: &str, args: &Value) -> ToolResult {
        let c = &self.client;
        match name {
            "session_open" => {
                if let Some(session) = &self.session {
                    return Ok(json!({ "session": session, "note": "a session is already open" }));
                }
                let record = c
                    .open_session(&self.principal, opt(args, "task").as_deref())
                    .map_err(failed)?;
                self.session = Some(record.id.clone());
                self.task = record.task.clone();
                self.shown_seq = 0;
                Ok(json!({ "session": record.id, "base": record.base, "task": record.task }))
            }
            "session_close" => {
                let session = self.session()?;
                c.close_session(&session).map_err(failed)?;
                self.session = None;
                self.task = None;
                Ok(json!({ "closed": session }))
            }
            "read" => {
                let session = self.session()?;
                let paths = list(args, "paths");
                if paths.is_empty() {
                    let path = arg(args, "path")?;
                    return match c.read(&session, &path).map_err(failed)? {
                        Some(bytes) => String::from_utf8(bytes)
                            .map(Value::String)
                            .map_err(|_| format!("{path} is a binary file")),
                        None => Err(format!("{path} does not exist")),
                    };
                }
                let mut text = String::new();
                for path in paths {
                    let body = match c.read(&session, &path).map_err(failed)? {
                        Some(bytes) => match String::from_utf8(bytes) {
                            Ok(body) => body,
                            Err(_) => {
                                text.push_str(&format!("=== {path} === (binary file)\n"));
                                continue;
                            }
                        },
                        None => {
                            text.push_str(&format!("=== {path} === (does not exist)\n"));
                            continue;
                        }
                    };
                    text.push_str(&format!("=== {path} ===\n{body}"));
                    if !body.ends_with('\n') {
                        text.push('\n');
                    }
                }
                Ok(Value::String(text))
            }
            "write" => {
                let session = self.session()?;
                let files: Vec<(String, String)> =
                    match args.get("files").and_then(Value::as_object) {
                        Some(files) => files
                            .iter()
                            .map(|(path, content)| {
                                content
                                    .as_str()
                                    .map(|content| (path.clone(), content.to_owned()))
                                    .ok_or_else(|| format!("content of {path} must be a string"))
                            })
                            .collect::<Result<_, _>>()?,
                        None => vec![(arg(args, "path")?, arg(args, "content")?)],
                    };
                let mut written = Vec::new();
                for (path, content) in files {
                    c.write(&session, &path, content.as_bytes())
                        .map_err(failed)?;
                    written.push(path);
                }
                Ok(json!({ "written": written }))
            }
            "append" => {
                let session = self.session()?;
                let path = arg(args, "path")?;
                let mut content = c.read(&session, &path).map_err(failed)?.unwrap_or_default();
                if !content.is_empty() && !content.ends_with(b"\n") {
                    content.push(b'\n');
                }
                let text = arg(args, "text")?;
                content.extend_from_slice(text.as_bytes());
                if !text.ends_with('\n') {
                    content.push(b'\n');
                }
                c.write(&session, &path, &content).map_err(failed)?;
                Ok(json!({ "appended": path }))
            }
            "delete" => {
                let path = arg(args, "path")?;
                c.delete(&self.session()?, &path).map_err(failed)?;
                Ok(json!({ "deleted": path }))
            }
            "list" => {
                let scope: Vec<String> = opt(args, "scope").into_iter().collect();
                let files = c.files(&self.session()?, &scope).map_err(failed)?;
                let lines: Vec<String> = files
                    .iter()
                    .map(|f| {
                        if f.changed {
                            format!("{} (changed)", f.path)
                        } else {
                            f.path.to_string()
                        }
                    })
                    .collect();
                Ok(Value::String(if lines.is_empty() {
                    "(no files)".into()
                } else {
                    lines.join("\n")
                }))
            }
            "land" => {
                let session = self.session()?;
                let key = format!("mcp-{session}-{}", self.next_key);
                self.next_key += 1;
                let receipt = c
                    .land(&session, &arg(args, "message")?, &key)
                    .map_err(failed)?;
                let mut completed = None;
                let next = match &receipt.outcome {
                    Outcome::Landed { .. } | Outcome::Unchanged { .. } => {
                        self.session = None;
                        let task = self.task.take();
                        if let (Some(task), Some(summary)) = (task, opt(args, "summary")) {
                            // The landing stands either way: report a failed completion
                            // next to the receipt instead of hiding the receipt.
                            completed =
                                Some(match c.complete_task(&task, &self.principal, &summary) {
                                    Ok(task) => json!(task.id),
                                    Err(e) => json!({ "task": task, "error": e.to_string() }),
                                });
                        }
                        "Session finished. Call task_next or session_open for more work."
                    }
                    Outcome::Conflicted { .. } => {
                        "Call conflict_show, then conflict_resolve, then land again."
                    }
                    Outcome::Rejected { .. } => {
                        "Call rebase, re-check the affected files, then land again."
                    }
                };
                Ok(json!({ "receipt": receipt, "completed": completed, "next": next }))
            }
            "rebase" => {
                let session = self.session()?;
                let outcome = c.rebase(&session).map_err(failed)?;
                if matches!(outcome, RebaseOutcome::AlreadyLanded { .. }) {
                    self.session = None;
                    self.task = None;
                }
                to_json(&outcome)
            }
            "intent" => {
                let ttl_seconds = args
                    .get("ttl_seconds")
                    .and_then(Value::as_i64)
                    .unwrap_or(600);
                let intent = Intent {
                    description: arg(args, "description")?,
                    reads: scopes(args, "reads")?,
                    writes: scopes(args, "writes")?,
                    exclusive: args
                        .get("exclusive")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    ttl_ms: ttl_seconds * 1000,
                };
                let session = self.session()?;
                let wait_ms = args
                    .get("wait_seconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    * 1000;
                // A blocking lock: the wait happens here, not in the agent's model turns.
                let started = Instant::now();
                let outcome = loop {
                    let outcome = c.declare_intent(&session, &intent).map_err(failed)?;
                    let waited = started.elapsed().as_millis() as u64;
                    if matches!(outcome, IntentOutcome::Admitted { .. }) || waited >= wait_ms {
                        break outcome;
                    }
                    std::thread::sleep(Duration::from_millis(LOCK_POLL_MS.min(wait_ms - waited)));
                };
                let admitted = matches!(outcome, IntentOutcome::Admitted { .. });
                let mut reply = to_json(&outcome)?;
                reply["waited_ms"] = json!(started.elapsed().as_millis() as u64);
                if admitted && args.get("rebase").and_then(Value::as_bool).unwrap_or(false) {
                    let rebased = c.rebase(&session).map_err(failed)?;
                    if matches!(rebased, RebaseOutcome::AlreadyLanded { .. }) {
                        self.session = None;
                        self.task = None;
                    }
                    reply["rebase"] = to_json(&rebased)?;
                }
                Ok(reply)
            }
            "notifications" => {
                let session = self.session()?;
                let notes = c.notifications(&session).map_err(failed)?;
                if let Some(last) = notes.last() {
                    self.shown_seq = self.shown_seq.max(last.seq);
                    if args.get("ack").and_then(Value::as_bool).unwrap_or(false) {
                        c.ack_notifications(&session, last.seq).map_err(failed)?;
                    }
                }
                to_json(&notes)
            }
            "conflicts" => to_json(&c.conflicts(true).map_err(failed)?),
            "conflict_show" => to_json(&c.conflict(&arg(args, "id")?).map_err(failed)?),
            "conflict_resolve" => {
                let mut resolutions = BTreeMap::new();
                for (path, text) in args
                    .get("resolutions")
                    .and_then(Value::as_object)
                    .ok_or("missing object argument `resolutions`")?
                {
                    resolutions.insert(path.clone(), text.as_str().map(str::to_owned));
                }
                to_json(
                    &c.resolve_conflict(&arg(args, "id")?, &resolutions)
                        .map_err(failed)?,
                )
            }
            "task_next" => {
                if let Some(session) = &self.session {
                    return Err(format!(
                        "session {session} is still open: land it (or session_close) before taking the next task"
                    ));
                }
                match c.claim_next(&self.principal).map_err(failed)? {
                    ClaimOutcome::Claimed(task) => {
                        let record = c
                            .open_session(&self.principal, Some(&task.id))
                            .map_err(failed)?;
                        self.session = Some(record.id.clone());
                        self.task = Some(task.id.clone());
                        self.shown_seq = 0;
                        Ok(json!({ "task": task, "session": record.id, "base": record.base }))
                    }
                    ClaimOutcome::AtLimit { holding } => Err(format!(
                        "you still hold {}: session_open with that task, land it with a summary (or task_complete / task_release)",
                        holding.join(", ")
                    )),
                    _ => Ok(Value::String("no open task".into())),
                }
            }
            "task_list" => to_json(&c.tasks().map_err(failed)?),
            "task_create" => to_json(
                &c.create_task(
                    &self.principal,
                    &arg(args, "subject")?,
                    &opt(args, "detail").unwrap_or_default(),
                    &scopes(args, "scope")?,
                )
                .map_err(failed)?,
            ),
            "task_claim" => to_json(
                &c.claim_task(&arg(args, "id")?, &self.principal)
                    .map_err(failed)?,
            ),
            "task_release" => to_json(
                &c.release_task(&arg(args, "id")?, &self.principal)
                    .map_err(failed)?,
            ),
            "task_complete" => to_json(
                &c.complete_task(
                    &arg(args, "id")?,
                    &self.principal,
                    &opt(args, "summary").unwrap_or_default(),
                )
                .map_err(failed)?,
            ),
            "log_append" => {
                let kind: EntryKind =
                    serde_json::from_value(json!(arg(args, "kind")?)).map_err(|e| e.to_string())?;
                let entry = NewEntry {
                    kind: Some(kind),
                    topics: list(args, "topics"),
                    scope: scopes(args, "scope")?,
                    body: arg(args, "body")?,
                    evidence: Vec::new(),
                };
                to_json(&c.log_append(&self.principal, &entry).map_err(failed)?)
            }
            "log_read" => to_json(
                &c.log_new(&self.principal, &list(args, "topics"))
                    .map_err(failed)?,
            ),
            "log_resolve" => {
                let seq = args
                    .get("seq")
                    .and_then(Value::as_u64)
                    .ok_or("missing integer argument `seq`")?;
                to_json(
                    &c.log_resolve(seq, &self.principal, &opt(args, "note").unwrap_or_default())
                        .map_err(failed)?,
                )
            }
            "leases" => to_json(&c.leases().map_err(failed)?),
            _ => Err(format!("unknown tool {name}")),
        }
    }
}

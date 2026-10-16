//! `tl`: the Trellis command line (spec §5.3). Every command prints JSON on stdout.

use std::error::Error;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;
use trellis_client::Client;
use trellis_client::checkout::{self, Checkout};
use trellis_coord::{ClaimOutcome, EntryKind, NewEntry, Scope, TaskStatus};
use trellis_land::{Intent, IntentOutcome, Outcome, Policy};

#[derive(Parser)]
#[command(
    name = "tl",
    version,
    about = "Trellis: collaboration infrastructure for agent swarms"
)]
struct Cli {
    /// Server URL.
    #[arg(
        long,
        global = true,
        env = "TRELLIS_SERVER",
        default_value = "http://127.0.0.1:7420"
    )]
    server: String,
    /// Repository name.
    #[arg(long, global = true, env = "TRELLIS_REPO", default_value = "main")]
    repo: String,
    /// Who is acting (an agent or person name).
    #[arg(
        long = "as",
        global = true,
        env = "TRELLIS_PRINCIPAL",
        default_value = "human"
    )]
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
        /// How many unfinished tasks one principal may hold at once.
        #[arg(long, default_value_t = 1)]
        max_claims: usize,
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
    /// Claim the oldest open tasks (up to --count, within the server's claim limit), open a
    /// session bound to the first, and check it out into DIR (refreshing an existing checkout to
    /// the latest main). Prints the tasks; an empty list when none is open.
    Start {
        dir: PathBuf,
        #[arg(long, default_value_t = 1)]
        count: usize,
    },
    /// Take exclusive locks (leases) on scopes for the checkout's session; they are released
    /// when it lands. With --wait, wait up to that many seconds for others' locks to go away;
    /// with --update, then sync, rebase onto the latest main, and refresh the files.
    Lock {
        dir: PathBuf,
        #[arg(required = true)]
        scopes: Vec<String>,
        #[arg(long, default_value_t = 0)]
        wait: u64,
        #[arg(long)]
        update: bool,
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
        /// Once landed, complete the session's task with this summary.
        #[arg(long)]
        summary: Option<String>,
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
    /// Serve MCP over stdio for one agent (`--as`), e.g. for Claude Code.
    Mcp,
    /// Bridge to a Git repository.
    Git {
        #[command(subcommand)]
        action: GitAction,
    },
}

#[derive(Subcommand)]
enum GitAction {
    /// Make main match a Git commit's tree (one changeset).
    Import {
        dir: PathBuf,
        #[arg(long, default_value = "HEAD")]
        rev: String,
    },
    /// Commit main's changesets not yet exported, one Git commit each.
    Export { dir: PathBuf },
}

#[derive(Subcommand)]
enum ConflictAction {
    Show {
        id: String,
    },
    /// Write each text conflict's merge, with markers, into the checkout's files.
    Markers {
        dir: PathBuf,
        id: String,
    },
    /// Resolve with the checkout's current file contents.
    Resolve {
        dir: PathBuf,
        id: String,
    },
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
    Claim {
        id: String,
    },
    /// Claim the oldest open task.
    Next,
    Release {
        id: String,
    },
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

/// Compact JSON: agents read this output, and every byte is context they pay for.
fn print<T: Serialize>(value: &T) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}

fn scopes(list: &[String]) -> Result<Vec<Scope>, Box<dyn Error>> {
    Ok(list
        .iter()
        .map(|s| Scope::parse(s))
        .collect::<Result<Vec<_>, _>>()?)
}

fn generated_key() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
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
        Command::Serve {
            addr,
            data,
            policy,
            max_claims,
        } => {
            let mut config = trellis_server::Config::new(addr, data);
            config.max_claims = max_claims;
            if let Some(path) = policy {
                let text = fs::read_to_string(path)?;
                config.policy = Policy::from_yaml(&text)?;
                config.validators = trellis_validate::from_yaml(&text)?;
            }
            trellis_server::run(config).map_err(|e| e.to_string())?;
        }
        Command::Init => print(&json!({ "repo": cli.repo, "main": client()?.init()? }))?,
        Command::Open { dir, task, scopes } => {
            let c = client()?;
            let session = c.open_session(&cli.principal, task.as_deref())?;
            let co = checkout::checkout(&c, &session.id, &cli.principal, &scopes, &dir)?;
            print(
                &json!({ "session": session.id, "base": session.base, "files": co.files.len() }),
            )?;
        }
        Command::Start { dir, count } => {
            let c = client()?;
            let mut tasks = Vec::new();
            while tasks.len() < count.max(1) {
                match c.claim_next(&cli.principal)? {
                    ClaimOutcome::Claimed(task) => tasks.push(task),
                    ClaimOutcome::AtLimit { holding } if tasks.is_empty() => {
                        return Err(format!(
                            "you still hold {}: land with --summary, or tl task complete / release",
                            holding.join(", ")
                        )
                        .into());
                    }
                    _ => break,
                }
            }
            let Some(first) = tasks.first() else {
                print(&json!({ "tasks": [], "note": "no open task" }))?;
                return Ok(ExitCode::SUCCESS);
            };
            let session = c.open_session(&cli.principal, Some(&first.id))?;
            let co = if dir.join(".trellis").exists() {
                checkout::rebind(&dir, &session.id)?
            } else {
                checkout::checkout(&c, &session.id, &cli.principal, &[], &dir)?
            };
            let tasks: Vec<_> = tasks
                .iter()
                .map(|t| json!({ "id": t.id, "subject": t.subject, "detail": t.detail }))
                .collect();
            print(&json!({ "tasks": tasks, "session": session.id, "files": co.files.len() }))?;
        }
        Command::Lock {
            dir,
            scopes: list,
            wait,
            update,
        } => {
            let co = Checkout::load(&dir)?;
            let c = co.client()?;
            let intent = Intent {
                description: format!("{} locks {}", co.principal, list.join(", ")),
                writes: scopes(&list)?,
                exclusive: true,
                ttl_ms: 600_000,
                ..Default::default()
            };
            // A blocking lock: the waiting happens here, not in the caller's turns.
            let started = Instant::now();
            let outcome = loop {
                let outcome = c.declare_intent(&co.session, &intent)?;
                let waited = started.elapsed().as_millis() as u64;
                if matches!(outcome, IntentOutcome::Admitted { .. }) || waited >= wait * 1000 {
                    break outcome;
                }
                std::thread::sleep(Duration::from_millis(200));
            };
            let admitted = matches!(outcome, IntentOutcome::Admitted { .. });
            let mut out = json!({
                "outcome": if admitted { "admitted" } else { "conflict" },
                "waited_ms": started.elapsed().as_millis() as u64,
            });
            if let IntentOutcome::Conflict(conflict) = &outcome {
                out["conflict"] = serde_json::to_value(conflict)?;
            }
            if admitted && update {
                out["update"] = serde_json::to_value(checkout::update(&dir)?)?;
            }
            print(&out)?;
            return Ok(if admitted {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(3)
            });
        }
        Command::Next { dir, task } => {
            let co = Checkout::load(&dir)?;
            let session = co.client()?.open_session(&co.principal, task.as_deref())?;
            let co = checkout::rebind(&dir, &session.id)?;
            print(
                &json!({ "session": session.id, "base": session.base, "files": co.files.len() }),
            )?;
        }
        Command::Status { dir } => {
            let (co, changes) = checkout::status(&dir)?;
            let notes = co.client()?.notifications(&co.session)?;
            print(&json!({ "session": co.session, "changes": changes, "notifications": notes }))?;
        }
        Command::Sync { dir } => print(&checkout::sync(&dir)?)?,
        Command::Update { dir } => print(&checkout::update(&dir)?)?,
        Command::Land {
            dir,
            message,
            key,
            summary,
        } => {
            checkout::sync(&dir)?;
            let co = Checkout::load(&dir)?;
            let c = co.client()?;
            let receipt = c.land(&co.session, &message, &key.unwrap_or_else(generated_key))?;
            let mut out = serde_json::to_value(&receipt)?;
            if let (Some(summary), Outcome::Landed { .. } | Outcome::Unchanged { .. }) =
                (summary, &receipt.outcome)
            {
                // Everything this principal holds was worked on in this checkout: the session's
                // task and any others `tl start --count` claimed with it.
                let mut done = Vec::new();
                for task in c.tasks()? {
                    if matches!(&task.status, TaskStatus::Claimed { owner, .. } if *owner == co.principal)
                    {
                        done.push(c.complete_task(&task.id, &co.principal, &summary)?.id);
                    }
                }
                out["completed"] = json!(done);
            }
            print(&out)?;
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
                TaskAction::Create {
                    subject,
                    detail,
                    scopes: list,
                } => print(&c.create_task(&cli.principal, &subject, &detail, &scopes(&list)?)?)?,
                TaskAction::Claim { id } => print(&c.claim_task(&id, &cli.principal)?)?,
                TaskAction::Next => print(&c.claim_next(&cli.principal)?)?,
                TaskAction::Release { id } => print(&c.release_task(&id, &cli.principal)?)?,
                TaskAction::Complete { id, summary } => {
                    print(&c.complete_task(&id, &cli.principal, &summary)?)?
                }
            }
        }
        Command::Log { action } => {
            let c = client()?;
            match action {
                LogAction::Add {
                    body,
                    kind,
                    topics,
                    scopes: list,
                } => {
                    let kind: EntryKind = serde_json::from_value(json!(kind))?;
                    let entry = NewEntry {
                        kind: Some(kind),
                        topics,
                        scope: scopes(&list)?,
                        body,
                        evidence: Vec::new(),
                    };
                    print(&c.log_append(&cli.principal, &entry)?)?;
                }
                LogAction::Read { topics } => print(&c.log_new(&cli.principal, &topics)?)?,
                LogAction::Resolve { seq, note } => {
                    print(&c.log_resolve(seq, &cli.principal, &note)?)?
                }
            }
        }
        Command::Leases => print(&client()?.leases()?)?,
        Command::Mcp => {
            let server = trellis_mcp::Server::new(client()?, cli.principal.clone());
            trellis_mcp::serve_stdio(server)?;
        }
        Command::Git { action } => match action {
            GitAction::Import { dir, rev } => print(&trellis_git::import(
                &client()?,
                &cli.principal,
                &dir,
                &rev,
            )?)?,
            GitAction::Export { dir } => print(&trellis_git::export(&client()?, &dir)?)?,
        },
    }
    Ok(ExitCode::SUCCESS)
}

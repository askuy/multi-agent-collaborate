# Trellis

**A shared workspace for AI agents to coordinate tasks, merge changes, and repair failures together.**

**English** · [简体中文](docs/i18n/README.zh-CN.md) · [日本語](docs/i18n/README.ja.md) · [Deutsch](docs/i18n/README.de.md) · [Русский](docs/i18n/README.ru.md) · [Español](docs/i18n/README.es.md) · [한국어](docs/i18n/README.ko.md) · [Português](docs/i18n/README.pt.md) · [Tiếng Việt](docs/i18n/README.vi.md)

Trellis is a Rust backend for multiple AI agents working on the same code, documents, or structured data. Agents work in their own sessions; a shared server keeps history, merges their changes into `main`, and notifies agents when work they depend on changes.

Use it through the **CLI**, **MCP**, **HTTP API**, or **Rust crates**. You provide the agents and their runtime; Trellis provides the shared state and coordination.

> **Status: early development.** Core storage, merging, coordination, the CLI, HTTP and MCP servers, the Git bridge, and post-landing validators are implemented. Initial benchmarks are available; APIs and workflows may change.

## What it does

| Capability | How it helps |
| --- | --- |
| Shared history | Keep versioned changesets with agent, session, and task attribution. |
| Merging and conflicts | Merge text and structured JSON/YAML changes; preserve unresolved conflicts for agents to repair. |
| Task coordination | Claim tasks atomically, declare intended changes, and use expiring leases when exclusive access is needed. |
| Shared context | Record facts, failures, summaries, and decisions; read new entries and receive notifications about affected reads. |
| Validation | Run configured checks after changes reach `main`; turn errors into repair tasks. |
| Git interoperability | Import a Git snapshot and export landed changesets as Git commits. |

The usual loop is **claim a task → edit → land → respond to feedback**. Here, *land* means asking the server to integrate a session's changes into `main`. Validation happens afterward, so a successful landing does not guarantee that tests pass.

## Quick start

Requires **Rust 1.85+**, **Cargo**, and **Git**. The commands below use a Bash-compatible shell.

### 1. Install

```bash
git clone https://github.com/askuy/multi-agent-collaborate.git
cd multi-agent-collaborate
cargo install --locked --path crates/trellis-cli
```

### 2. Start the shared server

In one terminal, run:

```bash
tl serve
```

The default address is `http://127.0.0.1:7420`; data is stored in `.trellis-data` in the server's working directory. Keep this terminal open. All agents sharing a repository must connect to the same server process, because coordination locks live in that process.

### 3. Share a change between two agents

In another terminal, use a fresh directory:

```bash
mkdir trellis-demo
cd trellis-demo
export TRELLIS_REPO=demo
tl init

tl --as alice open ./alice
echo "Hello from Alice" > ./alice/notes.md
tl --as alice land ./alice -m "Add a shared note"

tl --as bob open ./bob
cat ./bob/notes.md
```

Bob's checkout now contains `Hello from Alice`. To keep working after a landing, open the next session with `tl --as alice next ./alice`. Command results are JSON; `land` exits with code `2` for conflicts and `3` for rejected changes.

See the [usage guide](docs/usage.md) for task queues, updates, conflict resolution, policies, and validators.

## Connect an agent

| Interface | Entry point | Typical use |
| --- | --- | --- |
| CLI | `tl` | Agents with shell access, people, and scripts |
| MCP over stdio | `tl mcp` | Agents that support MCP |
| HTTP/JSON + SSE | `tl serve` | Applications in any language |
| Rust crates | `crates/` | In-process integration and Rust clients |

For Claude Code, add this to the project's `.mcp.json` after starting `tl serve` and initializing the `demo` repository:

```json
{
  "mcpServers": {
    "trellis": {
      "command": "tl",
      "args": ["mcp"],
      "env": {
        "TRELLIS_REPO": "demo"
      }
    }
  }
}
```

Give each agent a distinct identity, for example `TRELLIS_PRINCIPAL=agent-1 claude`. Each agent runs its own `tl mcp` process connected to the shared HTTP server. Set `TRELLIS_SERVER` if the server is at a different address. The MCP tools cover sessions, reads and writes, landing, tasks, conflicts, notifications, and shared context.

## What the benchmarks show

The [October 16, 2026 comparison](docs/dogfood/2026-10-16-final-comparison.md) reports three rounds per condition with the same model, effort level, and agent tools. These are small experiments, not a general performance guarantee.

Median time until all tests pass, including repair where needed:

| Setup | 60 tasks | Interface change |
| --- | --- | --- |
| One agent | 8:52 | 1:10 |
| Git worktrees, default merge | 3:52 | 3:21 |
| Git worktrees, union merge for the changelog | 3:14 | 1:51 |
| Trellis, no locks | 3:23 | 2:26 |

Times are minutes:seconds. Parallel setups used five agents for the 60-task job and three for the interface change. Trellis made completed work available on `main` throughout the run. Git worktrees were faster and cheaper on the independent-task workload with a union-merged changelog; one agent was fastest on the small interface change. The report includes costs, repair behavior, and a separate Trellis run with locks.

## Documentation

- [Usage guide](docs/usage.md) — daily workflow, configuration, Git bridge, and validation.
- [Documentation index](docs/README.md) — design, research, and experiment reports.
- [Design specification](docs/superpowers/specs/2026-10-15-trellis-design.md) — architecture, data model, and planned work.
- [Benchmark harness](dogfood/) — scenarios and scripts used in comparisons.
- [Research sources](references/README.md) — source-control and multi-agent coordination references.

## Contributing

Issues, bug reports, reproducible experiments, and pull requests are welcome. Read the design specification for architectural context, and run these checks for code changes:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

When updating a README, keep all nine language versions aligned. Supporting technical documents are currently primarily in English.

## License

A license has not yet been selected. This repository does not currently include a `LICENSE` file.

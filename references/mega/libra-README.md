[中文](README.zh-CN.md) | English

![Libra](docs/image/libra-banner.png)

<div align="center">

# Libra — An AI-native Extended VCS Built for Agents

**Versioning the entire software creation lifecycle, not just code.**

</div>

Libra is an AI-native version control system that combines Git-compatible code history with capture of external AI coding sessions. It records supported agents' session and checkpoint metadata and, when transcript content is available, stores redacted snapshots in `refs/libra/traces`, so developers can inspect the context behind changes and carry that history with a repository.

Libra fits into existing developer workflows through Git-compatible objects and wire protocols. Its capture hooks support Claude Code, Codex, and OpenCode; those tools run the coding agents, while Libra records their work.

<div align="center">

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/libra-tools/libra/actions/workflows/base.yml/badge.svg)](https://github.com/libra-tools/libra/actions/workflows/base.yml)
[![Discord](https://img.shields.io/badge/Discord-join-%235865F2?logo=discord&logoColor=white)](https://discord.gg/MTbb5rDYsC)
[![X](https://img.shields.io/badge/X-%40git_mono_AI-%231DA1F2?logo=x&logoColor=white)](https://x.com/git_mono_AI)
[![Docs](https://img.shields.io/badge/docs-docs.libra.tools-29B1FF)](https://docs.libra.tools)

</div>

---

## Key Differentiators

| Capability | Traditional VCS (Git) | Libra |
|-----------|----------------------|-------|
| **Versioned Artifacts** | Code history | Code history + captured agent checkpoints and redacted transcripts |
| **AI Collaboration** | Manual commit messages | External-agent session and checkpoint history |
| **Knowledge Reuse** | Code snapshots | Inspectable captured context alongside repository history |
| **Security** | External GPG/SSH setup | Built-in vault with per-repo key isolation |
| **Agent Integrations** | N/A | Capture hooks for Claude Code, Codex, and OpenCode |
| **Automation** | External CI/CD | Repository rules for cron and VCS events |

---

## Quick Start

### Install

```bash
# macOS (Apple Silicon) / Linux (amd64, arm64) — recommended
# Intel macOS has no prebuilt binary; build from source with `cargo build --release`.
# The installer verifies an Ed25519-signed release manifest and the binary's
# sha256/size before installing; see docs/installation.md for the trust model.
curl -fsSL https://download.libra.tools/install.sh | sh

# Homebrew (macOS)
brew install libra-tools/libra/libra

# From source (requires Rust)
git clone https://github.com/libra-tools/libra.git
cd libra
cargo build --release --locked
```

The source build writes `target/release/libra`; run that binary directly, or use
`cargo install --path . --locked` to install it in Cargo's binary directory.

The script installer also creates the optional shorthand
`~/.libra/bin/lba -> libra` as a relative symlink. Re-running the same version
repairs a missing alias without replacing the binary. Use `--no-alias` or
`LIBRA_NO_ALIAS=1` to opt out; an existing user-owned `lba` is never
overwritten. See [installer behavior and options](docs/installation.md).

#### Auto-upgrade (opt-in)

Official script installs can opt in to automatic upgrades:

```bash
libra config set --global upgrade.mode auto   # auto | manual | off (default off)
```

When `auto` is set, Libra checks a signed release manifest at
`https://download.libra.tools` alongside your normal commands and installs a
newer signed release in the background; the check is throttled, budget-limited,
and never affects your command's outcome. First-phase support is Linux
x86_64/aarch64 and macOS aarch64; Windows is published but auto-upgrade returns
`UnsupportedPlatform` and leaves the binary untouched. Third-party and manual
installs (Homebrew, from-source, package managers) never auto-upgrade and are
never marked official. Failed upgrades roll back to the previous version
automatically. The mode lives in `{LIBRA_HOME}/upgrade/settings.json`, not in
the SQLite config. See [auto-upgrade](docs/auto-upgrade.md).

### Initialize Your First Repository

```bash
# Create a new Libra repository
libra init my-project
cd my-project

# Or convert an existing Git repository
libra init --from-git-repository /path/to/existing/git/repo
```

### Use Agent Capture

```bash
# Enable capture hooks for Codex, then check the installation
libra agent enable --agent codex
libra agent list
libra agent doctor

# Run your agent tool normally — Libra captures sessions and checkpoints
codex

# Inspect captured sessions
libra agent session list
libra agent checkpoint list
```

> See the [agent command guide](docs/commands/agent.md) for supported agents, provider setup, and session management.

---

## Core Features

### 🧠 External Agent Capture & Trace History

`libra agent` records sessions and checkpoints from supported external coding agents. Checkpoint metadata is indexed locally; when transcript content is available, redacted snapshots are stored as repository objects reachable from `refs/libra/traces`. Use `libra agent session`, `checkpoint`, and `doctor` to inspect captured state, and `libra agent push` to share traces with a remote.

`libra review` and `libra investigate` run supported external agent CLIs in isolated workspaces for read-only reviews and investigations. `libra agent bridge --stdio` exposes the repository-scoped DeepSeek Harness bridge over JSON-RPC. See the [review](docs/commands/review.md), [investigate](docs/commands/investigate.md), and [agent](docs/commands/agent.md) guides.

The former `libra code` UI/runtime and `libra publish` site host were removed in 0.23.x. External-agent capture uses `libra agent`; repository backup uses `libra cloud`.

```
.libra/
├── libra.db              # SQLite: VCS state + agent capture metadata
├── vault.db              # Encrypted signing keys and credentials
├── objects/              # Object store (loose + pack, compatible with Git)
└── sessions/agent/       # Local agent capture event logs
```

### 🔄 Git-Compatible Foundation

Libra uses Git-compatible objects, index and pack formats, and supports Git remote workflows with servers such as GitHub, GitLab and Gitea. Command and flag coverage varies; consult the [compatibility matrix](COMPATIBILITY.md) for supported behavior and limitations.

Key difference: Git manages files. Libra manages **creation**.

### 🔐 Vault-Backed Security

By default, `libra init` creates a per-repository encrypted vault and generates a **GPG signing key**. Generate **SSH authentication keys** separately with `libra config generate-ssh-key --remote origin`. Signing and authentication keys are isolated per repository. See the [configuration guide](docs/commands/config.md).

### 🛡️ Sandbox Diagnostics

`libra sandbox status` reports the backend and enforcement available for Libra operations that request its internal sandbox. External coding agents keep their own execution and safety policies.

### ☁️ Tiered Cloud Storage & Backup

- **Tiered storage**: Offload large objects to S3/R2/RustFS with local LRU caching
- **Cloud backup**: Sync repository objects, references and captured agent history to Cloudflare D1 + R2
- **Portable**: Restore backed-up repository state and captured context on another machine

---

### Remote Mega2 browser

`libra mega2 browser --server http://127.0.0.1:9000` opens a terminal with a
directory tree on the left and read-only file content on the right. Expand
directories with Enter/Right, select a file to preview it, and use Tab to switch
panes. Previews highlight recognized code/configuration types; `--no-color`
disables syntax colors. Text previews are limited to 256 KiB. No clone or local repository is
required; `--list`/`--json` still return one directory level for scripts.
See the [browser guide](docs/commands/mega2.md) for keys and limits.
Use `a` to create a directory, `m` to move a file or directory, `e` to rename
a directory, or `t` then `a` to create a tag; these shortcuts do not require
Shift. Names and tag messages are entered in centered popup dialogs. Moving
uses a separate destination-folder picker: Right browses, the first Enter
chooses a destination, and the second Enter confirms; Esc goes back or cancels.

## Supported External Agents

The supported capture roster is **Claude Code**, **Codex**, and **OpenCode**. Use `libra agent list --schema-version 2 --json` to inspect installation state and import/export capabilities.

The current macOS ordinary OpenCode capture baseline is official **OpenCode 2.0.29** with **Libra 0.30.49**. Follow the [exporter-trust and capture-plugin setup](docs/commands/agent.md#opencode-ordinary-capture-on-macos-libra-03049); `opencode --pure` disables capture. Unsupported reasoning state and its associated text/time are omitted from a safe ordinary checkpoint, which remains `Unproven` / `Partial`. Historical imports containing unsupported reasoning state still fail with `LBR-AGENT-020`. This ordinary path does not admit native `signature`, `redactedData` or `reasoningEncryptedContent` artifacts or prove complete reasoning coverage; native source proof remains unproven.

> See the [agent command guide](docs/commands/agent.md) for provider setup, capture boundaries, and session management.

---

## Community & Resources

| Resource | Link |
|----------|------|
| **Website** | [libra.tools](https://www.libra.tools) |
| **Documentation** | [docs.libra.tools](https://docs.libra.tools) |
| **Discord** | [Join the community](https://discord.gg/MTbb5rDYsC) |
| **X / Twitter** | [@git_mono_AI](https://x.com/git_mono_AI) |
---

## Contributing

We welcome contributions from developers, AI researchers, and anyone passionate about the future of software creation. Before submitting a Pull Request, please ensure your code passes our quality checks:

```bash
# Run clippy with all warnings treated as errors
cargo clippy --all-targets --all-features -- -D warnings

# Check code formatting (requires nightly toolchain)
cargo +nightly fmt --all --check

# Fix formatting automatically if needed
cargo +nightly fmt --all

# Run the default acceptance suite
cargo test --all
```

For Windows installation, see the [Windows installation instructions](docs/installation.md#windows).

For detailed contribution guidelines, see [docs/contributing.md](docs/contributing.md).

---

## License

MIT License — see [LICENSE](LICENSE) for details.

Copyright (c) 2025-2026 Web3 Infrastructure Foundation.

Copyright (c) 2026 GitMono Limited.

---

<div align="center">

**[Get Started](https://docs.libra.tools/en/docs/getting-started) · [Join Discord](https://discord.gg/MTbb5rDYsC) · [Follow on X](https://x.com/git_mono_AI)**

</div>

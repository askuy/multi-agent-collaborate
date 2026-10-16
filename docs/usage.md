# Using Trellis

[Project overview](../README.md) · [Documentation index](README.md)

This guide assumes `tl` is installed and one shared `tl serve` process is running. The [quick start](../README.md#quick-start) covers installation and a first landing.

## Connection and identity

| Environment variable | CLI option | Default |
| --- | --- | --- |
| `TRELLIS_SERVER` | `--server` | `http://127.0.0.1:7420` |
| `TRELLIS_REPO` | `--repo` | `main` |
| `TRELLIS_PRINCIPAL` | `--as` | `human` |

Use a distinct principal for each agent. All agents sharing a repository must use the same server process: coordination locks are held in memory. `tl serve --addr HOST:PORT --data PATH` changes the listen address and storage directory.

CLI command results are JSON. Use `tl --help` or `tl <command> --help` for the available options.

## Sessions and working copies

`tl open DIR` opens a session and creates a working copy. Changes stay private until the server lands them on `main`.

| Command | Action |
| --- | --- |
| `tl status DIR` | Show local edits and pending notifications. |
| `tl sync DIR` | Send local edits to the session without landing. |
| `tl update DIR` | Sync edits, rebase onto the latest `main`, and refresh local files. |
| `tl land DIR -m "message"` | Sync and request integration into `main`. |
| `tl next DIR` | Open another session after the previous one has landed. |
| `tl notes DIR --ack` | Read and acknowledge notifications. |

For a sparse checkout, use `tl open DIR --scope 'src/**'`; repeat `--scope` to include more paths. Scopes support an exact path, a directory prefix ending in `/**`, or `**` for everything.

A landing returns an outcome such as `landed`, `unchanged`, `conflicted`, or `rejected`. The CLI exits with code `2` for a conflict, `3` for rejection, and `1` for other errors. Supply `--key KEY` to reuse the same idempotency key when retrying a landing request.

## Claim and complete tasks

With `TRELLIS_REPO` set to an initialized repository:

```bash
tl task create "Write the introduction" --scope 'docs/**'
tl --as alice start ./alice-task
# Edit files in ./alice-task.
tl --as alice land ./alice-task -m "Write the introduction" --summary "Introduction ready"
```

`start` claims the oldest available task and opens its session. Check the returned `tasks` list before editing: an empty list means no task is available. A successful landing with `--summary` also completes the claimed tasks. Run `start` again for the next task.

The server allows one unfinished claim per principal by default. To allow batches, start it with `tl serve --max-claims 4`, then use `tl start DIR --count 4`. Use `tl task list`, `tl task release ID`, or `tl task complete ID --summary "..."` to inspect or manage claims.

## Resolve conflicts

When `land` returns a conflict, use its ID in place of `CONFLICT_ID`:

```bash
tl conflicts
tl conflict show CONFLICT_ID
tl conflict markers ./alice CONFLICT_ID
# Edit the affected files in ./alice to resolve the conflict.
tl conflict resolve ./alice CONFLICT_ID
tl land ./alice -m "Resolve the conflict"
```

`markers` writes conflict markers for text conflicts. The resolution command uses the current contents of the affected files in the checkout. Review the resulting files before landing again.

When exclusive access is needed, `tl lock DIR 'path/**' --wait 30 --update` requests a lease, waits up to 30 seconds, and updates the checkout after acquisition. Inspect the returned outcome before editing. Locks are released when the session lands; `tl leases` lists active leases.

## Share context

```bash
tl --as alice log add "The API now uses integer cents" --kind decision --topic payments
tl --as bob log read --topic payments
```

Entry kinds are `fact`, `fail`, `summary`, `decision`, and `note`. Reads return entries the principal has not yet seen. Resolve a failure with `tl log resolve SEQ --note "How it was fixed"`.

## Import and export Git history

```bash
tl --as importer git import ./my-repo
# Agents work and land changes through Trellis.
tl git export ./my-repo
```

Import makes Trellis `main` match the Git repository's `HEAD` tree as one changeset; it does not import the full Git history. Use `--rev REV` to choose another revision.

Export creates one Git commit per unexported changeset, with agent authorship and `Trellis-Changeset`, `Trellis-Seq`, `Trellis-Session`, and `Trellis-Task` trailers where applicable. Export refuses a dirty worktree; import refuses when Trellis has changes that have not yet been exported.

## Configure merge policies and validators

Create `trellis.policy.yaml` and start the server with `tl serve --policy trellis.policy.yaml`:

```yaml
rules:
  - pattern: "canon/**"
    mode: strict
  - pattern: "Cargo.lock"
    mode: serialize
  - pattern: "CHANGELOG.md"
    merge: union
validators:
  - name: tests
    scope: ["src/**", "crates/**", "Cargo.toml", "Cargo.lock"]
    command: ["cargo", "test", "--quiet"]
    timeout_seconds: 900
    severity: error
```

- `strict` rejects a landing if `main` has changed the affected path since the session's base.
- `serialize` processes overlapping landings one at a time.
- `merge: union` keeps concurrent appended text from both sides, which can suit a changelog.

Adapt the validator command and scopes to the project stored in Trellis. A validator runs **after landing**, in a checkout of `main` at that changeset. Omit `scope` to run it for all changed paths. Environment variables include `TRELLIS_CHANGESET`, `TRELLIS_SEQ`, `TRELLIS_PRINCIPAL`, and `TRELLIS_CHANGED`.

An `error` produces a `FAIL` context entry and a repair task named `Fix <name> failure from seq N`. A `warning` produces a `NOTE` only. Find results with `tl log read --topic validator` and `tl task list`. Since checks do not gate landing, `main` can temporarily contain failing changes.

## Implementation map

| Crates | Responsibility |
| --- | --- |
| `trellis-core`, `trellis-store` | Changesets, content-addressed blobs, and SQLite metadata |
| `trellis-merge`, `trellis-derive` | Merge drivers and derived indexes |
| `trellis-land`, `trellis-coord` | Landing, conflicts, tasks, leases, and context |
| `trellis-server`, `trellis-client`, `trellis-cli`, `trellis-mcp` | HTTP, checkouts, command line, and MCP |
| `trellis-git`, `trellis-validate` | Git interoperability and post-landing checks |
| `trellis-sim` | Deterministic multi-agent simulation |

See the [design specification](superpowers/specs/2026-10-15-trellis-design.md) for changeset storage, compare-and-swap updates to refs, derived data, admission routing, and longer-term plans. The specification includes planned behavior as well as implemented mechanisms.

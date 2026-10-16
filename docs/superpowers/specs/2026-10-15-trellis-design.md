# Trellis design spec: a general-purpose Rust system for multi-agent collaboration

> Date: 2026-10-15 · Status: design spec v1 (pending user review) · Codename: Trellis (final name TBD)
> Sources: the Anthropic Scion article (main source) plus the papers and projects in `references/`. **Every mechanism in this spec is tagged with its source** ([§9 traceability matrix](#9-traceability-matrix)).
> Background: [research report](../../research/2026-10-15-multi-agent-collaboration-substrate.md)

---

## 1. Goals and boundaries

### 1.1 Goal

Provide **one general-purpose collaboration backend** for many agents (LLM agents, plus people and scripts) working on shared artifacts at the same time (code, documents, structured data). Any product can integrate it, through:

- a **Rust crate** (embedded, in-process),
- a **standalone server** (HTTP/JSON + SSE, any language),
- an **MCP server** (Claude Code / Codex / Cursor and other agents join directly),
- a **CLI `tl`** (people and scripts).

### 1.2 Design assumptions (from Scion)

1. **Clients are always online.** Agents run in the cloud or connected to the internet; offline-first support is not provided. [Scion: "Clients are always online"; Nick G.: "Claude is … necessarily online"]
2. **The server does the heavy work, and it is the only party that lands changes.** History, merging, rebasing, and derived queries all happen on the server; the client only keeps a bounded cache and a working copy. [Scion: "Client-server asymmetry"; ATM: neutral steward]
3. **Every query has an API.** Many tasks need no local copy at all. [Scion: "comprehensive source control service"]
4. **Commit cost is proportional to the size of the change, not the size of the repository; operations are lock-free.** [Scion: "cheap, incremental, and lock-free"]
5. **Working copies are optional and sparse.** [Scion: "working copy is optional, and sparse by design"]
6. **Discourage global mutable state.** Only `main` (plus a few release refs) is a global ref; agent sessions are not named branches. [Scion: "discourages refs … global state … is a looming scaling problem"]

### 1.3 Non-goals for v1

- Multi-node / distributed storage (Spanner/FoundationDB), regional cache tiers (the interfaces are kept so they can be added later).
- A full Git replacement (v1 only provides a Git **bridge**: import and export).
- Real-time co-editing of one document character by character (CRDT). Not in v1; the server-side three-way merge is enough.
- A built-in LLM runner, playbooks, and dashboard (v2; v1 lets agents connect themselves via MCP/CLI/HTTP).
- Live external state (Kubernetes, databases, and similar resources that cannot be buffered). CoAgent's MTPO is meant for those; v1 only governs **versioned files** (see the reasoning in §4.3).

---

## 2. Concept model

| Concept | Definition | Source |
|---|---|---|
| **Repo** | A versioned file namespace (one per product / project) | — |
| **Blob** | Immutable content addressed by BLAKE3, stored with zstd compression | Scion: content-addressed; Oak: BLAKE3 |
| **Changeset** | A write-ahead-log entry: **records only the files changed relative to the first parent** + blob pointers + provenance | Scion changeset; Mononoke Bonsai |
| **Ref** | A global pointer; v1 has only `main` and `release/*`; updates are **CAS on a single row** | Scion: "compare-and-swap on a single Spanner row" |
| **Session** | One agent's temporary workspace for one task: `base` (the main changeset it is based on) + `overlay` (unlanded writes) + `read_set` (what it actually read) | Scion: discouraging refs; ScorpioFS Antares (private upper layer / CL layer / shared lower layer); CoAgent read sets |
| **Land** | The server turns a session's overlay into a changeset and places it on main (rebase/merge + CAS) | Scion: server-side commit/rebase; ATM: the steward applies writes |
| **Task** | An item in the task queue: `open → claimed → done`, **the first claim wins, an agent holds at most one unfinished claim** | DeLM task queue; AgentRoom room_claim |
| **Lease** | An exclusive / shared claim on a resource pattern (path glob or logical key), with a TTL, **returning a deterministic conflict** | AgentRoom claim; MCP Agent Mail file reservations |
| **Intent** | A session's declared read/write footprint and admission mode before it writes | CoAgent footprint R(τ)/W(τ); ATM write intent |
| **Notification** | When a landed change intersects a session's read set, a message is pushed to that session describing the change; **the agent decides whether to repair** | CoAgent "Notify, do not lock or abort" |
| **Context log** | An append-only shared log of typed entries: FACT / FAIL / SUMMARY / DECISION, with scope and evidence; **a read returns only new entries since the last read** | DeLM shared context (FACT/FAIL/summary, demand paging) |
| **Conflict** | A first-class object: a write conflict that cannot be merged automatically is stored and assigned to a resolver instead of making the land "fail" | Jujutsu first-class conflicts; Scion: "Claude can typically fix these conflicts" |
| **Validator / Finding** | A check that runs after landing; its output is a Finding, which becomes a repair task | Scion post-merge testing; Atlas; DeLM FAIL entries |
| **Derived data** | Indexes computed from changesets (path history, tree index, read-set index, blame, …), versioned and computed eagerly or lazily | Scion derived data; Mononoke derived data (versioned mapping) |
| **Receipt** | The result of every mutating call: an explicit identity + confirmed/uncertain status + idempotency key | Oak "receipts with … confirmed/uncertain outcomes" |
| **Provenance** | Each changeset records principal, session, model, task, and the hash of the trace | Libra `refs/libra/traces`; Kition KDM Origin |

---

## 3. Architecture

```text
 Agents / products
  ├─ MCP (stdio / Streamable HTTP)   ├─ CLI `tl`   ├─ HTTP/JSON + SSE   └─ embedded Rust crate
                         │
┌──────────────────────── trellis-server (axum; stateless, can be scaled horizontally later) ────────────────────────┐
│ api: repo / session / read / write / land / task / lease / intent / context-log / conflict / finding / events   │
│ ┌─────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌─────────────┐ ┌──────────────────┐ │
│ │ trellis-land│ │ trellis-coord│ │trellis-derive│ │trellis-notify│ │trellis-     │ │ trellis-policy   │ │
│ │ admission → │ │ tasks/leases/│ │ derived-data │ │ read-set     │ │ validate    │ │ path ACL /       │ │
│ │ merge → CAS │ │ intents      │ │ framework    │ │ matching →   │ │ post-land   │ │ write policy     │ │
│ └─────────────┘ └──────────────┘ └──────────────┘ │ event fan-out│ │ → findings  │ └──────────────────┘ │
│        │                                         └──────────────┘ └─────────────┘                      │
│ trellis-merge (text / structured YAML,JSON / binary)      trellis-core (types, encoding, hashing)       │
└──────────────────────────────────────────────────────────────────────────────────────────────────────┘
                         │ MetaStore trait                         │ BlobStore trait
                 SQLite (WAL) [v1] / Postgres [v2]           local directory [v1] / object_store S3,GCS [v2]

 Client side: trellis-client (Rust) ── want-based fetch (identical RPCs) ── local object cache + sparse working copy (`tl checkout`)
 Bridge: trellis-git (gix) ── import (Git → changesets), export (main → Git commits)
 Testing: trellis-sim (deterministic simulation, fault injection)
```

---

## 4. Key mechanisms

### 4.1 Changesets and storage [Scion + Mononoke]

- `Changeset { id, parents[], seq?, changes: BTreeMap<Path, FileChange>, provenance, message, created_at }`
  - `FileChange = Upsert{blob, kind} | Delete`. (Rename is expressed as Delete + Upsert in v1; history continuity is maintained by a derived "path origin" index.)
  - **`changes` is relative to the first parent only.** [Scion footnote: "Storing changed files relative to the first parent of a merge is a stylistic choice"]
  - `id = BLAKE3(canonical encoding, excluding seq)`; `seq` is a monotonically increasing number on main (the generation).
- **Writing a changeset and moving a ref are separate.** Writing needs no locks; only the final step is a CAS on one row: `UPDATE refs SET target=?, version=version+1 WHERE name=? AND target=?`. [Scion]
- **There is no fully materialized tree.** "The full tree at changeset X" is derived data (a Mononoke-style Merkle tree index, `tree_index`), computed after landing and cached. [Scion: "no fully materialized copy of the repo anywhere"; Mononoke: diff-friendly representations are derived data]
- Single-changeset size limit (configurable, default 10k files / 64 MB), avoiding the cache problem of Mononoke's 5–6 MB Bonsai changesets.

### 4.2 Admission routing [ATM + AgentRoom + CoAgent]

When a session declares an Intent (optional but recommended) or lands, the server routes it according to the **write-set overlap and the resource policy** into one of four outcomes [ATM: "parallel admission, deterministic composition, serialization, or fail-closed"]:

| Outcome | Condition | Behavior |
|---|---|---|
| **Parallel** | The write set does not intersect any active exclusive lease, and no path changed since base intersects this write set | Land directly (fast-forward or pointer-only rebase) |
| **Compose** | The write sets intersect, but every intersecting file has an automatic merge driver and the merge succeeds | Server-side three-way merge, then land |
| **Serialize** | The resource's policy is `serialize` (for example a lockfile or schema migration), or the merge driver declares the file must be serialized | Queue for a per-resource serial queue; land in order |
| **Fail-closed** | Conflicts with someone else's **exclusive** lease; or a merge fails and the policy is `strict` | Return a Receipt with the holder or conflict details; **do not write** |

- If a merge fails and the policy is not strict: **create a Conflict object** (§4.6) and return `Conflicted{conflict_id}`; the session's work is not lost. [jj]
- Lease claims are **atomic and return a deterministic conflict** (`Ok | Conflict{holder, intent, expires_at}`), never "silently succeeding". [AgentRoom: "room_claim() returns a system-level conflict error" — the text-only coordination baseline failed 1 of 2 runs, while the CRDT-state AgentRoom succeeded 7 of 7]
- **Leases are advisory by default (advisory + logged)**: writing without a lease is allowed, but it is recorded in the context log for everyone to see; only resources marked `exclusive` in the policy are enforced (fail-closed). [AgentRoom: "advisory-plus-apology … strict locking"; Chubby-style advisory locking]

### 4.3 Read-set tracking and notifications: "Notify, do not lock or abort" [CoAgent + Scion]

**Why we don't lock and don't abort:** on 10 contended workloads, CoAgent measured that 2PL deadlocks 0.81 times per trial and gets almost no speedup (1.04×); OCC aborts 0.95 times per trial, is slower than serial (0.93×), and costs 1.83× the tokens (up to 2.33×); uncoordinated execution passes only 13%. Notify-based recovery passes all of them with a **1.4× speedup and 1.15× token cost**. The cause: "a single agent transaction spans minutes of inference, read sets are broad and opaque". Scion likewise gave up the global lock of a merge queue and moved to post-merge testing.

**Our approach (MTPO simplified for versioned files):**

1. **Snapshot isolation**: reads in a session default to `base + overlay`. Writes go to the overlay first and **do not affect anyone else** (this is CoAgent's "private buffer" that live state lacks, so we don't need its write-ordering undo machinery).
2. **Read-set recording**: the server records `read_set` (paths and versions) as reads happen in the session; an Intent can declare a broader footprint ahead of time (globs). [CoAgent: footprint by declaration]
3. **Notification on landing**: whenever any changeset lands on main, the `notify` module matches it against every active session's read set and footprint. On a match, it pushes `Notification{changeset, paths, summary, author}` to that session (SSE / MCP notification / the next tool call's response carries it). [CoAgent: "the framework appends a notification to that agent's context describing what changed"]
4. **The agent decides**: the agent judges whether its premises are invalidated. If not, it ignores the notification. If they are, it calls `session.rebase()` (moves base to the latest main and updates the overlay with a three-way merge) and repairs only the affected parts. [CoAgent capabilities (i)(ii)]
5. **Stale reads when landing**: by default **landing is not rejected**. The receipt includes `stale_reads[]` and a notification is created automatically (repairs can be done in a follow-up changeset). Only resources with policy `strict-read` reject it (rare, for example releases).
6. Every notification and how it was handled (ignored / rebased / repaired) is recorded, for evaluation and auditing.

### 4.4 Landing algorithm (the steward) [Scion + ATM + Oak]

```text
land(session, idempotency_key):
  0. If idempotency_key already has a result → return the same Receipt                    [Oak receipts]
  1. policy check (path ACL / write policy)                                                 [Scion directory permissions]
  2. Build C from the overlay (parent = session.base)
  3. loop (≤ N retries):
       M ← main
       touched ← paths_changed(C.parent .. M)      # derived: per-changeset path sets + skip list   [Scion skip lists]
       W ← C.paths
       route ← admission(W, touched, leases, policy)                                           [ATM]
       Parallel  → C' = C with parent rewritten to M (only the parent pointer and id change)  [Scion: cost ∝ change]
       Compose   → C' = merge3 for each path in W ∩ touched (by FileKind)
       Serialize → enqueue in the resource queue and wait for its turn (still in this loop)
       FailClosed / merge failed under strict → return Receipt{Rejected|Conflicted}
       if CAS(main: M → C'.id): break
  4. Write seq; enqueue eager derived data; trigger notify (§4.3) and validators (§4.7)
  5. Return Receipt{Landed, changeset, seq, stale_reads, notifications_sent, idempotency_key}
```

- Throughput: the CAS on main is the only serialization point. When contention exceeds a threshold, enable **group commit** (put several candidates landing at the same time into one batch, merge them in order, and CAS once).
- **Every write is idempotent and has an explicit result.** On timeout the client retries with the same key and must never get a "duplicate landing".

### 4.5 Shared context [DeLM + NstAgent + Agentsensus]

Two kinds of shared state, both versioned:

1. **Context log** (append-only, entries are immutable) [DeLM]
   - Entry types: `FACT` (a finding with evidence), `FAIL` (a contradicted approach, or a defect in someone else's result; **stays open until resolved**), `SUMMARY` (completion summary of a subtask), `DECISION` (an agreed decision), `NOTE`.
   - Fields: `scope` (paths / topics), `evidence` (changeset/blob references), `attachments` (files are snapshotted at publication time; peers `expand` them to see diffs and `apply` them into their own overlay, **refusing to overwrite unaccounted local changes**). [DeLM expand/apply]
   - **Reads return only new entries and status changes since the last read**; older entries are fetched on demand (demand paging). [DeLM]
   - Defects are never fixed by editing an entry: append a `FAIL` + a repair task. Before submitting, an agent must handle every open FAIL related to its scope. [DeLM]
   - Topic subscriptions: entries are tagged with topics/scope, and agents subscribe on demand. **There is no global broadcast.** [AgentRoom: broadcast overhead rises super-linearly with N; quality peaks at N=2]
2. **Canon (typed state files)** [NstAgent + Agentsensus]
   - Ordinary versioned files with `kind = canon/yaml|json` + a JSON Schema; **merged structurally field by field**; entities are keyed by a stable `id`. (For example, a novel's character states, a product's API contracts, a project's ADRs.)
   - Updates are expressed as structured operations (`upsert / add / resolve`, matching NstAgent's `upsert_character_state / add_past_event / add_future_requirement / resolve_future_requirement`), and the server converts them into file changes.
   - Duplicate entries for the same entity are merged automatically and their source references kept. [Agentsensus: merging cuts records by 22–44%; turning it off multiplies the store by 3.1×]
   - Canon paths can be set to `exclusive` (only specific roles may land changes) or `review` (changes need approval).

### 4.6 Conflicts [jj + Scion]

- `Conflict { id, session, base, ours, theirs, paths[{path, base_blob, ours_blob, theirs_blob, hunks}], status: Open | Assigned(principal) | Resolved(changeset) }`.
- Resolution: whoever is assigned (the original author by default, or a dedicated resolver role) fetches the three versions → writes the resolution → `conflict.resolve` → lands as a normal changeset.
- Scion's experience: "concurrent changes can conflict, but Claude can typically fix these conflicts faster than humans can even identify them" → conflicts are a work item, not an error.

### 4.7 Validation after landing [Scion + Atlas + DeLM]

- Validators are registered per path prefix: `command` (a local or remote command, given the changeset and the materialized directory, for example `cargo test`, `mdbook build`, schema validation), `webhook`, and `llm` (v2).
- Output: `Finding { changeset, validator, severity, locations, message, evidence }`, written to the context log as a `FAIL` and **automatically creating a repair task**.
- Optional policies: `block_release` (block a release ref), `auto_revert` (automatically revert on a high-severity finding by creating an inverse changeset).
- "Superfans": watcher agents subscribe to a component's topic and findings and follow up continuously. [Scion: Claude Tag superfans] (v1 provides the subscription capability; the watchers themselves are user-side agents.)

### 4.8 Derived data framework [Scion + Mononoke]

- `trait Derivable { const NAME; const VERSION; fn derive(cs, parents_derived) -> Self; mode: Eager | Lazy }`; storage key `(repo, changeset, name, version)`. [Mononoke: `(repo_id, cs_id, derived_data_type, derived_data_version)`; bumping the version enables re-derivation]
- **Defined recursively**: each value is computed from the parents' derived values + this changeset. [Scion; Mononoke]
- Built into v1: `path_set` (eager, the set of paths changed by each changeset), `skip_list` (eager, ancestor jumps for fast merge_base and range queries), `tree_index` (eager, Merkle directory tree), `path_history` (lazy), `blame` (lazy).
- **The client never depends on the internal format**: everything goes through APIs, so formats can be optimized freely. [Scion]

### 4.9 Fetching and working copies [Scion]

- Object API: `GET /objects/{hash}` (blob/tree, immutable, cacheable forever) and `POST /objects/want` (batch).
- **Want-based negotiation**: the client starts from the root tree of the target changeset, checks its local cache, and **requests only the trees/blobs it is missing**, recursing level by level; sparse = it simply doesn't request subtrees outside its scope. [Scion: "operates exclusively on trees and blobs the client wants … Sparse … checkouts don't need special support"]
- **The server, a future regional cache, and the local daemon all serve the identical RPC**, so the client doesn't need to know the topology. [Scion: "identical RPCs"]
- `tl checkout --session S --paths 'src/**,docs/**' DIR`: materialize base + overlay into a directory; `tl sync`: scan the directory for differences → write to the overlay (v1 is an explicit command; v2 adds a file watcher, then FUSE/lazy mounts [Scion virtualized checkouts; Oak; ScorpioFS]).
- **Equal-tree refreshes don't touch unchanged files**, keeping compiler caches warm. [Oak]

### 4.10 Git bridge [Scion migration practice]

- `tl git import <repo> [--branch main]`: replay Git history into changesets (first-parent only; merge commits record the changes relative to the first parent). [Scion: the git→Scion ingestion pipeline]
- `tl git export <dir> [--since seq]`: write landed changesets to Git as commits (trailers carry session/task/principal), so code review, CI, and the existing ecosystem keep working.
- v1 is **one-directional and continuous**: main → Git. Pushes from the Git side are imported via `tl git import --incremental`, but if they conflict with Trellis main, they are reported, not merged automatically.

### 4.11 Permissions and audit [Scion + Mega/saturn]

- v1: policy file `trellis.policy.yaml`: `path glob → {write: roles[], mode: optimistic|serialize|exclusive|strict-read|review}`.
- v2: Cedar (`cedar-policy`), approval rules that distinguish change types (for example "only touched comments" vs "touched logic"). [Scion: "differentiate simple things like comment-only changes from sensitive logical changes"]
- Audit: all receipts, notifications and their handling, lease events, and conflicts are written to an append-only event table and exported via SSE.

---

## 5. Interfaces

### 5.1 HTTP (v1, JSON; events over SSE)

| Group | Endpoints |
|---|---|
| repo | `POST /repos` · `GET /repos/{r}` · `GET /repos/{r}/refs` |
| read | `GET /repos/{r}/files?path=&at=` · `GET …/tree?path=&at=` · `GET …/history?path=` · `GET …/diff?from=&to=` · `GET …/merge-base?a=&b=` · `GET …/changesets/{id}` |
| objects | `GET /objects/{hash}` · `POST /objects/want` · `POST /objects` (upload blob) |
| session | `POST …/sessions` (task?, base?, intent?) · `GET/DELETE …/sessions/{s}` · `PUT …/sessions/{s}/files` · `POST …/sessions/{s}/rebase` · `POST …/sessions/{s}/land` |
| coord | `POST …/tasks` · `POST …/tasks/{t}/claim` · `POST …/tasks/{t}/complete` · `POST …/leases` · `DELETE …/leases/{l}` · `POST …/leases/{l}/renew` · `POST …/intents` |
| context | `POST …/log` · `GET …/log?since=cursor&topics=` · `POST …/log/{e}/resolve` (for FAIL entries) |
| conflict / finding | `GET …/conflicts?assignee=` · `POST …/conflicts/{c}/resolve` · `GET …/findings?status=` |
| events | `GET …/events?session=&topics=` (SSE: landed / notification / lease / task / finding / conflict) |

Every mutating endpoint accepts `Idempotency-Key` and returns a `Receipt`.

### 5.2 MCP tools (v1)

`session_open` · `session_close` · `read` · `list` · `search` (v1: path + text substring) · `history` · `diff` · `write` · `patch` · `rebase` · `land` · `task_create` · `task_claim` · `task_complete` · `task_list` · `lease_acquire` · `lease_release` · `intent_declare` · `log_append` · `log_read` (new only) · `log_resolve` · `notifications` · `conflicts` · `conflict_resolve` · `findings`

- Tool descriptions are written for agents; error results come back as a structured `is_error` result with machine-readable `reason` + `holder` / `conflict_id` and similar fields. [AgentRoom: deterministic, machine-readable conflicts]
- **Every tool response carries pending notifications** (`_notifications`), so even clients that don't support MCP notifications won't miss them. [CoAgent: append to the agent's context]

### 5.3 CLI `tl`

`tl serve` · `tl repo create` · `tl session open|close` · `tl checkout` · `tl sync` · `tl land` · `tl task …` · `tl lease …` · `tl log …` · `tl conflicts` · `tl git import|export` · `tl mcp` (stdio MCP). All commands support `--json`.

### 5.4 Rust crate (embedded)

```rust
let tl = trellis::Embedded::open(Config::sqlite("data/"))?;
let s = tl.repo("app")?.session(SessionOpts::task("T-12"))?;
s.write("docs/a.md", bytes)?;
let receipt = s.land(LandOpts::idempotency("k1"))?;
```

---

## 6. Storage (v1 SQLite schema overview)

`repos` · `refs(name, target, version)` · `changesets(id, repo, seq, parents, provenance, message, created_at)` · `changeset_files(changeset, path, op, blob, kind)` · `derived(repo, changeset, name, version, value_blob)` · `sessions(id, repo, principal, base, task, status, created_at)` · `session_files(session, path, op, blob, kind)` · `session_reads(session, path, version)` · `tasks(id, repo, subject, detail, status, owner, created_by, …)` (`UNIQUE` constraint + conditional `UPDATE` for atomic claims) · `leases(id, repo, pattern, mode, holder, expires_at)` · `intents` · `log_entries(id, repo, kind, scope, topics, body, evidence, status, cursor)` · `read_cursors(principal, repo, cursor)` · `conflicts` · `findings` · `receipts(idempotency_key, …)` · `events(seq, …)`.

Blobs live in `data/objects/xx/xxxx…` (zstd). The `MetaStore` / `BlobStore` traits leave room to swap in Postgres or S3 later.

---

## 7. Correctness invariants (must be covered by the simulation tests)

1. **No lost writes**: once a `Landed` receipt is returned, its changeset is reachable from main forever.
2. **Lease exclusivity**: at any moment, at most one holder of an exclusive lease on a given resource.
3. **Idempotency**: requests with the same idempotency key return the same receipt and cause only one state change.
4. **Exclusive task claims**: at most one owner per task; at most one unfinished claim per agent (configurable).
5. **Complete notifications**: every landed change that intersects a read set reaches every active session involved (none missed).
6. **Conflicts are not lost**: every Conflicted receipt corresponds to an `Open|Assigned|Resolved` conflict object.
7. **Derived data is consistent**: derived values for the same (changeset, name, version) are deterministic; `tree_index` equals a naive replay.
8. **CAS linearizability**: the history of main is a single chain (first-parent), and `seq` is strictly increasing.

---

## 8. Testing strategy

| Layer | Content | When |
|---|---|---|
| Unit / property | `proptest`: canonical encoding / hashing, merge3 properties, structured merge, admission routing tables | Every commit |
| **Deterministic simulation** | `trellis-sim`: N virtual agents (scripted / random strategies, **no LLM**), fault injection (crash while holding a lease, latency/reordering, duplicate requests, CAS storms, timeouts and retries), checking §7 invariants; failures print a reproducible seed [Scion: "hermetic simulations of every non-trivial … interaction"] | Every commit (small); nightly (large: 200 agents × 10 minutes) |
| Integration | HTTP / MCP / CLI end to end; real Claude Code connected over MCP doing a small task | Per PR (no LLM) / manually |
| Fuzzing | `cargo-fuzz`: request decoding, merge drivers, Git import | Nightly |
| Dogfooding | Our own coding agents (multiple Claude Code instances) collaborating on real repositories through Trellis; record notification/conflict/finding statistics | From M3 on, continuously |
| Benchmarks | Single agent / naive parallel / Git worktrees / Trellis × N = 1…20; metrics: wall-clock time, tokens, conflicts, stale reads, number of validation findings | M4 |

---

## 9. Traceability matrix

| Mechanism | Main source | Supporting sources | Where we deliberately differ |
|---|---|---|---|
| Changeset records only first-parent changes | Scion | Mononoke Bonsai | — |
| Single-row CAS on refs, separated from writing changesets | Scion | Oak hosted merge CAS; ATM base-hash guarded apply | SQLite conditional UPDATE instead of Spanner |
| Discourage refs; sessions are not branches | Scion | Oak's per-session branches (we don't name them) | — |
| Derived data (recursive, versioned, eager/lazy) | Scion | Mononoke versioned mapping, 8 indexes | — |
| Want-based negotiation, identical RPCs on every tier | Scion | — | v1 has only server ↔ client |
| Sparse / virtual working copies | Scion | Oak lazy mount, ScorpioFS Antares | v1 has explicit checkout/sync only; FUSE in v2 |
| Server-side landing / neutral steward | Scion | ATM | — |
| Four admission outcomes | ATM | CoAgent | ATM's atoms/virtual atoms are simplified to "file + merge-driver regions" in v1 |
| Atomic claims, deterministic conflict returns, advisory + logged | AgentRoom | MCP Agent Mail (TTL, pre-commit guard) | exclusive resources are enforced |
| Notify instead of lock/abort; agents repair themselves | CoAgent | Scion post-merge + Claude fixes conflicts | No MTPO serialization order or saga inverses (we have a private overlay, so it isn't needed) |
| Typed context log, new-only reads, FAIL until resolved | DeLM | AgentRoom broadcast log | Topic subscriptions instead of a single log |
| Task queue: first claim wins, ≤1 unfinished claim | DeLM | AgentRoom | — |
| Structured state (canon) + structured update operations | NstAgent | ConWriter, Magnet/Atlas | Generalized to any domain's typed files |
| Merge and deduplicate shared memory | Agentsensus | — | Deduplicate within the canon merge driver |
| Validation after landing → repair tasks | Scion | Atlas, DeLM FAIL | — |
| First-class conflicts | Jujutsu | Scion | — |
| Receipts / idempotency | Oak | — | — |
| Provenance / traces | Libra | Kition KDM Origin | — |
| Per-directory permissions, change-type-aware approval | Scion | Mega saturn (Cedar) | Simple policy file in v1, Cedar in v2 |
| Deterministic simulation as the main testing approach | Scion | turmoil/madsim | — |
| Superfan watcher agents | Scion | — | v1 only provides the subscription capability |

---

## 10. Crate layout

```text
crates/
  trellis-core      types, canonical encoding, BLAKE3, path normalization
  trellis-store     MetaStore/BlobStore traits + SQLite/local-directory implementations
  trellis-merge     merge3 (text), structured merge (YAML/JSON + schema), binary
  trellis-derive    derived data framework + path_set/skip_list/tree_index/path_history/blame
  trellis-land      admission routing, landing steward, group commit
  trellis-coord     tasks, leases, intents
  trellis-notify    read-set matching, notification fan-out, event table
  trellis-context   context log (FACT/FAIL/SUMMARY/DECISION/NOTE), cursors, canon operations
  trellis-validate  validator registration and execution, findings → tasks
  trellis-policy    policy file parsing and checks
  trellis-server    axum HTTP + SSE (composition root)
  trellis-client    Rust client: want-based fetch, local cache, checkout/sync
  trellis-mcp       MCP server (rmcp)
  trellis-git       Git import/export (gix)
  trellis-cli       `tl`
  trellis-sim       deterministic simulation and fault injection
  trellis           facade crate (Embedded API)
```

Layering rules (enforced in CI): `server/mcp/cli` → `land/coord/notify/context/validate/derive` → `store/merge/core`; the domain layer must not depend on axum/rmcp.

---

## 11. Milestones (refined later in the implementation plan)

| Milestone | Content | Exit criteria |
|---|---|---|
| **M0** | core + store + merge + derive(path_set/skip_list/tree_index) | Property tests pass; tree_index == naive replay |
| **M1** | land (four admission outcomes, CAS, idempotency, conflict objects) + `trellis-sim` | Invariants 1/3/6/7/8 hold for 200 virtual agents |
| **M2** | coord + notify + context + policy | Invariants 2/4/5 hold; notification latency p99 < 100 ms (single node) |
| **M3** | server + MCP + CLI + client (checkout/sync) + Git bridge | Real Claude Code ×3 complete a task in a small repository through MCP; export to Git |
| **M4** | validate + dogfooding + benchmarks | Run continuously on our own repositories for a week; benchmark report |
| v2 | Built-in runner/playbooks, dashboard, FUSE, Cedar, Postgres/S3, LLM validators, cache tier | — |

---

## 12. Open questions (decisions to make before or during implementation)

1. **Git position**: is a "Trellis main → Git one-directional export" plus incremental import enough for v1 (recommended), or does it need true two-directional sync?
2. **Notification delivery**: should MCP rely on "every tool response carries pending notifications" (most compatible), and should we also use MCP's native notifications?
3. **Default task-claim limit**: DeLM uses "≤1 unfinished claim"; should it be configurable per repo?
4. **License**: Apache-2.0 (consistent with Kition / Mega)?
5. **Final name**: keep the Trellis codename for now, and check crates.io / GitHub name conflicts before release.

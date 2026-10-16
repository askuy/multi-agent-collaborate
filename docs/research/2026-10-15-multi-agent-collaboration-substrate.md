# Research report: a version control and collaboration layer for large-scale multi-agent work

> Date: 2026-10-15 · Status: research draft (v0.1), for later product design and implementation
> Working name: **Trellis** (temporary; it's the trellis that lets many vines grow in order along a shared frame)
> Local copies of the sources: [`references/`](../../references/README.md)

---

## 0. Conclusions up front

1. **Scion is worth learning from, but don't copy it wholesale.** Scion solves two problems at once: scale (100 GB of history, 25 million checkouts a day) and high-concurrency writes (30,000 merges a day). In the scenario of 200 agents writing one novel, the total text is only a few MB. **Storage scale is not the bottleneck. The bottlenecks are coordination, consistency, and context delivery.** What we should take from Scion is its *architectural principles*: a server-centric design, an API-first approach with no checkout required, cheap changesets, compare-and-swap (CAS) ref updates, derived data, and verification after merging. Its three-tier cache and Spanner layer can wait.
2. **The research is clear: coordination, not parallelism, decides quality.** AgentRoom (ICML 2026) found that a CRDT can merge bytes but cannot merge meaning. Without a claim mechanism, concurrency is *worse* than a single agent, and with a single broadcast channel, quality peaks at N=2 and broadcast overhead grows faster than linearly past ×3. **Collaboration among 200 agents must use layered, topic-subscribed coordination with explicit intents (read/write sets).**
3. **Shared context must be structured, versioned data.** NstAgent found that from 20K to 100K words, a structured narrative state (character state, past events, pending foreshadowing) clearly beats rolling summaries. Agentsensus found that merged shared memory is 22–44% smaller than per-agent memory. So the "story bible" should be **typed files plus derived indexes**, versioned together with the prose.
4. **Mega is the closest open-source reference implementation, but it's not a good base to fork.** Mega is a Git-compatible, open-source take on Piper, focused on the code forge (CLs, merge queue, Buck2 builds, code review). Its core is the Git object model, which runs against our goals: commit cost depends on tree depth, and it can't represent typed state. **Recommendation: write a new core in Rust using the Mononoke/Scion changeset model, and borrow Mega/Libra/ScorpioFS ideas and some crates locally (Cedar permissions, object-storage abstraction, a three-layer overlay, agent traces).**
5. **Product positioning:** not "a better Git", but a **versioned shared workspace for agent swarms**. It includes versioned storage, intent and lease coordination, a context-serving API, a post-merge validation pipeline, and an MCP interface. It is domain-agnostic: code, novels, research reports, game design docs, and so on.

---

## 1. Problem definition

### 1.1 From "code version control" to a "collaboration layer for agents"

The user's judgment: Claude-style projects will increasingly run in the cloud, and the work won't only be writing code. One example is **200 agents writing a novel together**. When agents work concurrently, the novel's context (world setting, character state, plot lines) and the coordination information *between* agents become the key problems.

Let's walk through that scenario and see what breaks if we take the most obvious approaches.

| Approach | Where it breaks |
|---|---|
| Shared folder / object storage | No version history, no atomicity. Concurrent writes silently overwrite each other (AgentRoom calls this "silent overwrite"). You can't answer "which version of the facts did this chapter rely on?" |
| One Git worktree per agent + PRs | Every commit has to rewrite the tree; the `main` ref becomes a global hotspot; a merge queue can't keep up at agent scale (Anthropic says so directly). Conflicts are only *text* conflicts. A semantic contradiction like "Chapter 12 kills a character, Chapter 15 brings them back" is invisible. |
| Real-time CRDT co-editing on one document | Strong eventual consistency only guarantees no bytes are lost, not that the meaning is consistent. AgentRoom measured a collision probability of about 0.20 for two agents editing the same file, and collisions often break the output. |
| A shared chat channel / message bus | Text claims break down: when two agents both announce a claim at once, neither notices the conflict. Broadcast overhead grows faster than linearly past N=3 (AgentRoom). |

### 1.2 A rough load estimate (novel scenario)

- Scale: 200 chapters × 5,000 words ≈ 1 million words. Prose is about 3–6 MB. The story bible (characters, places, timeline, foreshadowing) is about 1–10 MB.
- Writes: 200 agents each landing a change every ~2 minutes ≈ **1.7 changesets/second**. That's below Scion's average (30,000/day ≈ 0.35/s, but with much higher peaks).
- Reads: each agent step has to assemble context. With 200 agents × a few calls per minute, **reads outnumber writes by 1–2 orders of magnitude, and every read is "slicing" (only what's relevant to the task)**.
- Real costs: **tokens** (context assembly), **wasted work** (writing on stale facts and then rewriting), and **consistency errors**.

> The design goal is not "how many PB can we handle" but **"how few tokens and how little wasted work it takes for each agent to have just the right, fresh context, and to not step on anyone when it writes."**

---

## 2. Scion: key points from the original article (verified against the source)

Original article: `references/articles/anthropic-redesigning-source-control.md` (converted from HTML). The two details below that third-party coverage reported were **checked against the original text and confirmed**:

- ✅ "moving one [ref] is a compare-and-swap on a single Spanner row that happens separately from creating changesets"
- ✅ The git→Scion ingestion pipeline was rewritten by Claude Mythos Preview: "consumed 20x less memory and was 4x faster, while preserving all of the implicit invariants"

### 2.1 Technical details that matter for us (original text plus my interpretation)

| Scion fact | Original text, abridged | Implication for us |
|---|---|---|
| Changeset is the unit of work | "an entry in a write-ahead log … stores a list of the files that changed relative to its first parent, along with content-addressed pointers" | **Writing a commit costs O(size of the change).** Rebasing onto a new parent is also cheap when the touched paths don't overlap. This is the foundation for lock-free landing at high concurrency. |
| Creating changesets is separate from moving refs | "no packfiles … no locking or cross-server coordination, and no refs to move around" | Writing content can be fully parallel; only the final ref CAS is serialized. |
| Discourage refs | "discourages refs … except for the main ref or release branches, because global state … is a looming scaling problem" | **Agent sessions shouldn't create named branches.** Represent them with a temporary "session → base changeset" mapping. |
| Derived data | blame, path history, commit-graph skip lists; "choose whether to compute it eagerly or defer"; "client never sees the internals" | **Our "story index" (where entities appear, the timeline, character state at chapter N, summaries) is derived data.** |
| Negotiation only asks for "what I want" | "doesn't tell the server what the client already has; … operates exclusively on trees and blobs the client wants" | Content-addressed GETs can be cached by any layer, are resumable, and support sparse/virtual views naturally. |
| Identical RPCs on every tier | "daemon, cache, and server all respond to identical RPCs" | Get the API right from day one, and adding a cache tier later is free. |
| No offline support | Nick G. (HN): "Claude is … necessarily online" | We also assume agents are always online. |
| Abandoning the merge queue | "haven't been able to … replicate … a global repository lock on the merge queue … invested in post-merge and pre-deploy automated testing … Claude can typically fix these conflicts faster" | **Optimistic landing + post-merge validation + automatic fix tasks** instead of serialized pre-merge gates. |
| Quality practices | hermetic simulation, fuzzing, rate limiting; "superfans" (Claudes each watching one component) | Build the deterministic simulation harness from day one; "superfan" agents become a product feature. |
| Security | per-directory permissions; review rules that tell "comment-only changes" apart from "sensitive logic changes" | Per-path ACLs plus approval rules that know the change type. |

### 2.2 Precedents and disputes (to cite when writing it up)

- Simon Willison: it "sounds … like a reinvention of centralized source control along the lines of Subversion". The Register likewise compares it to SVN/Perforce and mentions Piper, VFS for Git, and Sapling.
- **sunshowers (a former Mononoke engineer)** points out that Scion's "changeset + derived data" closely matches Mononoke's **Bonsai changeset + derived data**. Mononoke eventually had 8 derived indexes; its blame went from O(repository) to O(touched files). They argue Anthropic only acknowledged "hard-won lessons" and didn't name Mononoke. → **For our implementation, the most direct prior art is Mononoke (open source, in `facebook/sapling` under `eden/mononoke`, Rust).**
- Cursor Origin (August 2026 beta) took a different path: **keep Git, replace GitHub**, and become a forge for the agent loop. A useful contrast for product positioning.

---

## 3. Related projects

### 3.1 Mega (gitmono-dev/mega) — the project the user found

- Positioning: "an open-source implementation of Google Piper — a Git-compatible monorepo engine built for the AI Agent era". Rust (edition 2024), MIT/Apache-2.0, about 535 stars, last push 2026-09-16.
- Workspace crates (see `references/mega/docs_architecture.md`):

| Crate | Role | Value to us |
|---|---|---|
| `mono` | HTTP/SSH server binary, REST + Git Smart HTTP | Low (we don't need Git compatibility as the main protocol) |
| `ceres` | Domain layer: Git transport, CLs, merge queue, builds, webhooks; communicates through an event bus | **Medium**: the layering discipline (transport/application split, event bus, CI-enforced dependency rules) is worth copying |
| `jupiter` / `callisto` / `jupiter-migrate` | Storage over SeaORM (Git trees/commits stored as database rows) | Low: the Git object model doesn't fit our changeset model |
| `io-orbit` | Object storage abstraction (local / S3 / GCS) | **Medium**: same idea as the `object_store` crate; can use it directly |
| `saturn` | Cedar policy engine | **High**: per-path ACLs and approval rules can use `cedar-policy` directly |
| `vault` | PGP/PKI/secrets | Low (not needed at first) |
| `orion*` | Buck2 build runner / scheduling | Low (code-specific) |
| `moon` | Next.js frontend | Low |

- Related projects:
  - **Libra** (agent-side Git client, SQLite storage): now focused on **capturing AI coding sessions**. It stores redacted transcripts in `refs/libra/traces` and supports Claude Code / Codex / OpenCode hooks. → **Inspiration: every changeset should be bound to its agent session trace (provenance).**
  - **ScorpioFS** (FUSE): its **Antares three-layer overlay** has a private writable upper layer, an optional CL layer, and a shared read-only lower layer. → **This maps exactly onto an agent workspace: session overlay / unlanded change / main snapshot.**
- Mega's own roadmap includes IntentSpec, multi-agent DAG orchestration, and code attribution. The direction overlaps with ours, but its center of gravity is the code forge.
- **Why not fork it**: (1) Git object model: every commit rewrites the tree from root to leaf, and rebase is expensive. (2) A large surface area (code review, builds, CLA, bots) is code-specific. (3) No typed state, intents/leases, or context API. Forking would mean inheriting a lot of code we'd then have to delete.

### 3.2 Sapling / Mononoke / EdenFS (Meta)

- Mononoke: Rust server; "built … to scale up to accepting thousands of commits every hour across millions of files"; the open-source version includes mysql/sqlite/file/S3 backends, but **Meta says it is not supported for external use**.
- Lessons that carry over:
  - Bonsai changesets are great for pushes but bad for diffs. Diff-friendly structures (a Git-like Merkle tree) **are themselves derived data**, computed asynchronously off the critical path.
  - Derived-data mappings carry a version: `(repo_id, cs_id, derived_data_type, derived_data_version)`, which makes re-derivation possible.
  - Very large changesets (tens of thousands of files, 5–6 MB) can't go into memcache. Watch the size of a single changeset.

### 3.3 Google Piper

- Centralized, trunk-based, about 25,000 engineers, CitC (a cloud-based client workspace, similar to a virtual checkout). Paper: CACM 2016, *Why Google Stores Billions of Lines of Code in a Single Repository* (ACM blocks scripted downloads; see the manual download note in `references/README.md`).

### 3.4 Oak (oakvcs, Rust, Apache-2.0)

- Content-addressed (BLAKE3 + content-defined chunking), SQLite per repo, **lazy mount** (FSKit/FUSE/ProjFS), **one branch per session**, unlimited commits that squash on landing.
- Lessons from the Dev log #2 (2026-10):
  - **Every mutating command returns a "receipt"** with an explicit identity and a confirmed/uncertain result. → Agents need idempotent, verifiable write results.
  - Merge uses CAS on the resolved heads, so concurrent merges and pushes can't lose acknowledged work.
  - Review without a checkout (`--remote` pins immutable heads and fetches only the content it needs).
  - On a partial history, a merge base is "either correct or reported unavailable".
  - **They removed the "software factory" orchestration layer.** → A reminder for us: build the substrate well before doing orchestration.
- Caution: performance claims are self-reported, and people on HN questioned them.

### 3.5 Jujutsu (jj)

- An **operation log** (every operation is recorded and can be undone); concurrent operations are detected and merged automatically; first-class conflicts (conflicts can be committed and resolved later). → **"A conflict is an object, not a failure"** is very useful for agent concurrency: land the conflict first, then assign a resolution task.

### 3.6 CRDTs (Loro / Automerge / Yrs)

- Loro 1.0: a Rust CRDT with checkout/fork/shallow snapshots ("make your JSON data collaborative and version-controlled").
- **Conclusion: don't use a CRDT as the core consistency model.** AgentRoom's data already shows that "SEC merges bytes but not semantics". A CRDT can be added later as an optional mode for real-time co-editing of a single document (for example, a human and an agent polishing the same chapter at the same time).

### 3.7 MCP Agent Mail (Rust/Python)

- Identities, inboxes, threads, **advisory file leases (glob + TTL)**, and a pre-commit guard, all on Git + SQLite.
- Lessons: leases are advisory and expire automatically, so crashed agents don't deadlock; requesting a lease returns the conflict instead of blocking; messages are stored outside the agent's context window.

---

## 4. Research on multi-agent collaboration (and the design principles it implies)

| Paper | Key findings | Design implication |
|---|---|---|
| **AgentRoom** (arXiv 2608.23740, ICML 2026) | CRDT-shared filesystem + 5 MCP tools (`room_claim/release/state/broadcast/read`). Coordination is the main driver of quality; with no coordination, concurrency (0.456) < single agent (0.544) < full AgentRoom (0.669); quality peaks at N=2, and broadcast overhead rises super-linearly past ×3; two agents significantly reduce "stub-and-exit" abandonment | Claims must be **atomic, deterministically return conflicts, and be enforced in the state layer**, not depend on the LLM reading text; **no global broadcast** |
| **CoAgent** (arXiv 2606.15376) | Applies database concurrency control to multi-agent systems: read/write sets, OCC validation, pessimistic locks as a fallback; distinguishes data / semantic / ordering conflicts | **Intents declare read and write sets**; OCC by default, exclusive leases for critical resources |
| **ATM** (arXiv 2607.00041) | Pre-write admission: maps write intents to semantic units ("atoms"); a neutral steward applies every shared write; supports deterministic composition / serialization / fail-closed | **The server is the only writer** (agents submit, the server lands); conflict-handling policy can be configured per resource |
| **DeLM** (arXiv 2606.10662) | Shared context + task queue; agents claim tasks asynchronously and publish findings right away; up to +17.5 points accuracy, 2.49× faster | Built-in **task board** + publish as soon as something is found |
| **NstAgent** (arXiv 2609.35759) | Structured narrative state (characters / past events / future requirements), updated once per chapter (upsert/add/resolve); from 20K–100K words it clearly beats rolling summaries; cost ≈ linear, $0.13–0.17 per 10K words | **The canon is typed state**, plus "pending foreshadowing" as trackable obligations |
| **Agentsensus** (arXiv 2609.32297) | Merging records of the same event into a single shared memory: 22–44% fewer entries; turning merge off makes it 3.1× larger | Canon facts should be **deduplicated and shared**, not each agent keeping its own memory |
| **Magnet + Atlas** (arXiv 2607.00918) | Character agents act on a shared world state; Atlas compares world-state graphs between scenes to catch hallucinations | **A continuity validator** as a post-landing step |
| StoryWriter / Agents' Room / ConWriter | Outline → plan → write in layers; shared scratchpad; neuro-symbolic consistency control | A role hierarchy (showrunner / planner / writer / checker) |

**Design principles we take from this:**

1. **The server is the single source of truth and the only one who lands changes** (Scion + ATM).
2. **Optimistic by default, explicit when critical**: content writes use OCC (validate the base when landing); canon, outlines, and other critical resources use exclusive leases (CoAgent + Agent Mail).
3. **Coordination signals are structured, deterministic, and enforced in the state layer** (AgentRoom).
4. **Notifications are driven by read sets, not by broadcast**: you are only notified when something you *read* changes (similar to cache-coherence invalidation).
5. **The canon is typed and versioned; prose is free text**, and both live in the same changeset history.
6. **Conflicts are first-class objects** (jj): landing doesn't lose work; conflicts get assigned to resolvers.
7. **Validate after merging, then repair automatically** (Scion's post-merge testing + Atlas's checker).
8. **Every write is idempotent and returns a receipt** (Oak).
9. **Everything can be traced back to an agent session** (Libra traces).

---

## 5. Comparing approaches

| | A. Fork Mega and add a coordination layer | **B. New Scion/Mononoke-style core (recommended)** | C. Coordination layer only (on Git/GitHub) |
|---|---|---|---|
| Core model | Git objects + CL | Changeset + content-addressed blobs + derived data | Git |
| Typed state / context API | Has to be added | Native | Lives outside, separate from history |
| Concurrent landing | Git tree rewrite, rebase is expensive | O(change size), server-side rebase is cheap | Limited by the forge |
| Time to first prototype | Medium (have to understand a big codebase) | Medium | **Fastest** |
| Long-term ceiling | Bound to the shape of a code forge | **Highest** | Low (can't solve consistency or context) |
| Git compatibility | Native | Via an export bridge (one-way at first) | Native |

**Recommendation: B.** For a minimum viable product (MVP), you can reuse ideas from C (lease and message semantics in the style of Agent Mail) and selected crates from A (`cedar-policy`, `object_store`, `gix` for the Git bridge).

---

## 6. Recommended architecture (B)

### 6.1 Overall architecture

```text
            ┌──────────────────────── Agents (Claude Agent SDK / Managed Agents / Claude Code / anything) ──────────────────────┐
            │       MCP (primary interface)        │        CLI `tl`        │      HTTP/JSON + SSE      │   (later) FUSE lazy mount   │
            └──────────────────────────────────────┴────────────────────────┴───────────────────────────┴────────────────────────────┘
                                                           │
┌──────────────────────────────────────────────── trellis-server (stateless, horizontally scalable) ─────────────────────────────────────────────┐
│  Session API     Read API          Context API         Coordination              Land Pipeline                 Events                  │
│  open/close      read/list/        context_pack        intents (read/write sets)  rebase → merge drivers →      subscribe(topics/     │
│  overlay write   history/blame/    (budget, task)      leases (TTL)               CAS main → enqueue derive     read-set) / inbox     │
│                  search/at=cs                          task board / channels      & validators                                        │
│                                                                                                                                       │
│  Derived Data Engine (versioned, eager/lazy, keyed by content)        Validator Pipeline (schema / continuity LLM / custom) → findings → tasks │
│  ACL (Cedar)  ·  Provenance (agent/model/session trace)  ·  Rate limit / Quotas  ·  Receipts & Idempotency                              │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
                     │ metadata (changesets/refs/leases/events)                         │ content-addressed blobs (zstd)
            Postgres (production) / SQLite (single binary)                     object_store: local FS / S3 / GCS
            (later: FoundationDB / TiKV)                                         (later: regional cache tier, same RPCs)
```

### 6.2 Data model

```rust
/// Content address: BLAKE3-256 over canonical bytes.
pub struct Hash([u8; 32]);

/// Immutable, content-addressed. Stored zstd-compressed in object storage.
pub struct BlobRef { hash: Hash, size: u64 }

/// A write-ahead-log entry. Records changes only (relative to the first parent), never the full tree.
pub struct Changeset {
    id: Hash,                       // hash of the canonical encoding (excludes seq)
    parents: Vec<Hash>,             // first parent = main line
    seq: Option<u64>,               // monotonic sequence number once landed on main (generation)
    changes: BTreeMap<RepoPath, FileChange>,
    author: Principal,              // agent id / human id
    provenance: Provenance,         // session_id, model, prompt/trace hash, intent_id
    message: String,
    created_at: Timestamp,
}

pub enum FileChange {
    Upsert { blob: BlobRef, kind: FileKind },   // FileKind determines the merge driver and validators
    Delete,
    Rename { from: RepoPath },                  // optional; can start as Delete+Upsert
}

pub enum FileKind { Text, Markdown, CanonYaml { schema: SchemaId }, Json { schema: Option<SchemaId> }, Binary }

/// The only global mutable state. Updates are always CAS: UPDATE … WHERE name=? AND target=?
pub struct Ref { name: RefName, target: Hash, version: u64 }

/// A session = a temporary workspace for one agent's single task. Not a named branch.
pub struct Session {
    id: SessionId, principal: Principal,
    base: Hash,                      // the main changeset it is based on (snapshot isolation)
    overlay: BTreeMap<RepoPath, FileChange>, // unlanded writes (the Antares "private upper layer")
    intent: Option<IntentId>,
    read_set: ReadSet,               // records paths/entities it actually read (used for invalidation and OCC)
}

pub struct Intent {                 // intent declaration (CoAgent/ATM)
    id: IntentId, session: SessionId, description: String,
    reads: Vec<PathPattern>, writes: Vec<PathPattern>,
    mode: LeaseMode,                // Optimistic | Exclusive | Shared
    ttl: Duration,
}

pub struct Conflict {              // a first-class conflict (jj)
    id: ConflictId, session: SessionId, base: Hash, theirs: Hash,
    paths: Vec<(RepoPath, ConflictDetail)>, status: Open | Assigned(Principal) | Resolved(Hash),
}

pub struct Finding {               // validator output
    id: FindingId, changeset: Hash, validator: ValidatorId, severity: Severity,
    locations: Vec<Location>, message: String, suggested_task: Option<TaskSpec>,
}
```

**Key points:**
- A changeset records only the changed files, so **rebasing onto a new parent when no paths overlap only rewrites the parent pointer and the hash. Cost O(change size)**. This is what makes landing at high concurrency possible.
- "The full tree / directory listing at changeset X" is **derived data** (a Mononoke-style Merkle tree index), computed asynchronously and cached.
- `FileKind` turns "a typed canon" into a file-level property without breaking the uniform "everything is a file" model. That's friendly to Git export and to people reading the repo.

### 6.3 Write path: Land (optimistic, lock-free, server-side)

```text
land(session):
  1. Admission: ACL check; does the intent's write set conflict with someone else's Exclusive lease? → reject with the conflicting party
  2. Build the changeset C from the overlay (parent = session.base)
  3. loop (bounded retries):
       M = read ref main
       if M == C.parent:                     → candidate = C
       else:
         touched_since = paths_changed(C.parent..M)   // uses derived data: skip list + per-changeset path sets
         overlap = touched_since ∩ C.paths
         if overlap is empty:                → candidate = C rebased onto M (only the parent changes)
         else for each p in overlap: merge_driver[kind(p)].merge3(base, ours, theirs)
              all succeed                    → candidate = merged changeset
              any fail                       → create Conflict, return Receipt{Conflicted, conflict_id}
       stale_reads = session.read_set ∩ touched_since   // OCC: the facts you read were changed by someone else
       if stale_reads violates the policy (e.g., Exclusive/Strict) → return Receipt{Stale, details}
       if CAS(main, M → candidate) succeeds: break
  4. Asynchronously: compute derived data (eager types) → run validators → publish events (invalidate subscribers whose read sets intersect)
  5. Return Receipt{Landed, changeset_id, seq, stale_reads(warn), idempotency_key}
```

- **Idempotency**: `land` takes an `idempotency_key`; if a retry hits a landed result, it returns the same receipt.
- **Throughput**: CAS on main is the only serialization point. 1.7/s is easy. If it reaches hundreds per second, use **group commit** (stack candidates from several sessions in one batch and do a single CAS).
- **Policy**: per path prefix, choose `optimistic` (default), `strict-read` (reject if a read went stale), or `exclusive` (requires a lease).

### 6.4 Merge drivers (by FileKind)

| FileKind | Strategy |
|---|---|
| Text / Markdown | Line-level three-way merge (`diffy::merge`). Markdown can first split by heading/scene into blocks, then merge, to lower the false-conflict rate |
| CanonYaml / Json | **Structural three-way merge**: field-level; list elements matched by a stable `id`; touching the same field → conflict; schema validation runs after the merge |
| Binary | Conflict whenever both sides changed it |

**Semantic conflicts** (for example, two separate files that contradict each other's facts) can't be caught by merge drivers. They're handled by three layers: (1) read-set invalidation notifications (prevention); (2) strict-read policy (gate at landing); (3) continuity validators (after merging).

### 6.5 Read path and the Context API

- Every read can pin `at=<changeset>` (snapshot isolation). By default it reads the session base + overlay.
- APIs: `read / list(prefix) / history(path) / blame(path) / search(query, scope) / diff(a, b) / merge_base(a, b)`. These are all server-side, **no checkout needed**.
- **`context_pack(task, budget_tokens)`** is the core differentiating API:
  - Inputs: a task description or task ID, a token budget, and optional must-include paths.
  - Process: pull from derived data the **canon entities relevant to the task** (current state), **open obligations** (unresolved foreshadowing), **neighboring chapter summaries**, **recent changes that affect your read set**, and the conventions/style guide. Rank, trim to the budget, and assemble.
  - Output: an ordered list of sections. **The stable prefix goes first** (conventions → canon → summaries → task), which helps prompt-cache hit rates.
  - Records what was actually returned into the session `read_set`.
- **LLM-computed derived data**: chapter summaries, entity extraction, and embeddings are also derived data, **cached by blob hash**. The same content is summarized only once, and all 200 agents share it.

### 6.6 The coordination layer

- **Intents/leases**: `intent_declare(reads, writes, mode, ttl)` → `Admitted | Conflicts([...holder, intent description, expiry])`. Leases renew with a heartbeat, are released automatically on expiry, and can be force-released by an admin.
- **Task board**: `task_create / task_claim / task_complete`. Tasks can be bound to paths and findings. Validator findings automatically generate repair tasks.
- **Channels**: messages are namespaced by topic (`/arc/2`, `/canon/characters/alice`, `/chapters/12`). **Agents subscribe on demand, and there is no global broadcast**; messages live in storage and agents fetch them with `inbox()`, so they don't eat context.
- **Read-set invalidation**: when a landed changeset touches a path in someone's read set, push an `Invalidated{paths, by_changeset}` event to that session. This is how we avoid a lot of wasted work.

### 6.7 Validation and operations ("superfans")

- The validator pipeline is triggered by events after landing and can be configured per path prefix:
  - Schema validation (canon files).
  - Continuity validation (LLM): extract fact assertions from the new chapter → compare them with canon at that changeset → output findings with locations.
  - Custom validators (word counts, banned words, style).
- Findings → tasks (assigned to the original author or to a dedicated repair agent). A severe finding can trigger an **automatic revert** (configurable).
- "Superfan" agents: each one watches a single piece (a story arc, a character, the system's health) and subscribes to the related topics.

### 6.8 Permissions, auditing, provenance

- `cedar-policy`: per-path / per-role ACLs. For example, `canon/**` can only be landed by the canon-keeper role; writers can only write `chapters/<assigned>/**`.
- Approval rules that know the change type (Scion): "only changed prose" vs. "changed canon fields".
- Provenance: each changeset records the session, model, a hash of the prompt/trace, and the intent. Traces are stored as blobs, similar to Libra's `refs/libra/traces`.

### 6.9 Agent interfaces

**MCP tools (draft):**

| Group | Tool | Description |
|---|---|---|
| Session | `session_open(task?, base="main")`, `session_close` | Returns session_id, base |
| Read | `read`, `list`, `search`, `history`, `diff`, `context_pack` | All can take `at` |
| Write | `write(path, content)`, `patch(path, unified_diff)`, `canon_upsert(entity, fields)` | Writes go to the overlay; canon writes get schema validation |
| Intent | `intent_declare`, `intent_renew`, `intent_release` | Deterministically returns conflicts |
| Land | `land(message, idempotency_key)` | Returns a receipt: Landed / Conflicted / Stale / Rejected |
| Coordination | `task_*`, `post(topic, msg)`, `subscribe(topics)`, `inbox()` | |
| Conflicts | `conflicts_assigned()`, `conflict_resolve(id, resolution)` | |

- CLI `tl`: for humans/scripts, with JSON output for everything.
- HTTP/JSON + SSE: the ground truth API; MCP and CLI are thin wrappers.
- Later: FUSE/FSKit lazy mounts (in the style of ScorpioFS/Oak) for tools that need a real filesystem (compilers, for example).
- Later: Git export bridge (`gix`): replay main into a Git repository, for humans and the existing ecosystem.

---

## 7. Novel scenario walk-through

### 7.1 Repository layout

```text
/style/guide.md                      # style conventions (stable prefix)
/canon/world/*.yaml                  # world setting (CanonYaml, schema: world)
/canon/characters/<id>.yaml          # character state: location / goals / relationships / knowledge / possessions / emotions (NstAgent)
/canon/timeline.yaml                 # event timeline
/canon/obligations.yaml              # open obligations: foreshadowing / promised confrontations / unresolved questions (with id, planted_in, due_by)
/outline/book.md                     # overall outline
/outline/arcs/<n>.md                 # per-arc outlines
/chapters/<nn>/scene-<k>.md          # prose (split by scene to lower conflict granularity)
/notes/<agent>/...                   # private scratch space (not validated)
```

### 7.2 Roles (with 200 agents)

| Role | Count | Permissions | Responsibilities |
|---|---|---|---|
| Showrunner | 1 | Everything | Overall outline, task breakdown, final call |
| Arc leads | ~10 | `outline/arcs/<n>`, task board | Detailed arc outlines, assigning chapters |
| Canon keepers | ~5 | `canon/**` (Exclusive) | Keep facts unique, approve canon change requests |
| Writers | ~150 | `chapters/<assigned>/**` | Write scenes, propose canon changes (as tasks/requests) |
| Continuity checkers / superfans | ~30 | Read-only + findings | One per character/arc; continuity checks, follow up on repairs |
| Editors | A few | `chapters/**` (rules) | Style unification, cross-chapter polishing |

### 7.3 A typical flow

1. A writer runs `task_claim("ch12/scene3")` → `session_open` → `intent_declare(writes=[chapters/12/scene-3.md], reads=[canon/characters/{alice,bob}.yaml, outline/arcs/2.md])`.
2. `context_pack(task, 30k)` → style guide + current state of alice/bob + obligations for arc 2 that are due + summaries of ch11/ch13 + the scene outline.
3. Writing in progress. A canon keeper lands a change to `alice.yaml` (her arm was injured in Chapter 10) → the writer receives `Invalidated` → re-reads and adjusts.
4. `land` → three-way merge → CAS → Landed receipt.
5. Continuity validator: finds "Chapter 12 has Alice lifting a heavy object with both hands" contradicting `injured_arm` → a finding → a repair task assigned back to the writer.
6. The writer finds that the plot needs a new piece of foreshadowing → `task_create(kind=canon_change_request)` → a canon keeper approves → lands it into `obligations.yaml`.

---

## 8. Rust tech stack and crate breakdown

| Purpose | Choice |
|---|---|
| Async / HTTP | `tokio`, `axum` (JSON + SSE); gRPC later via `tonic` if needed |
| Storage | `sqlx` (Postgres / SQLite); blobs via `object_store` (local / S3 / GCS) |
| Hashing / compression / encoding | `blake3`, `zstd`; canonical encoding via `postcard` or a hand-written stable encoding (don't use JSON for hashing) |
| Diff / merge | `imara-diff` (diff), `diffy` (three-way text merge); our own structural YAML/JSON merge (`serde_yaml`/`serde_json` + JSON Schema validation `jsonschema`) |
| Search | `tantivy` (BM25); vectors later |
| Permissions | `cedar-policy` |
| MCP | `rmcp` (official Rust MCP SDK) |
| Git bridge | `gix` (gitoxide) |
| Testing | `proptest`, deterministic simulation `turmoil` / `madsim`, `cargo-fuzz` |
| Observability | `tracing` + OpenTelemetry |
| LLM calls (validators / summary derivation) | Anthropic Messages API (no official Rust SDK, so `reqwest` + SSE directly). Default `claude-opus-5`, with effort set per role; cheaper workers `claude-sonnet-5` / `claude-haiku-4-5` chosen by benchmark; non-real-time evaluation via the Batches API. Behind a pluggable provider trait |

```text
crates/
├── trellis-core        # Hash, Changeset, FileChange, canonical encoding, RepoPath
├── trellis-store       # trait MetaStore / BlobStore + sqlite / postgres / object_store implementations
├── trellis-derive      # derived data framework (versioned mappings, eager/lazy, content-keyed caching)
├── trellis-merge       # merge drivers: text / markdown-block / structured (yaml/json)
├── trellis-land        # landing pipeline: rebase / merge / OCC / CAS / group commit
├── trellis-coord       # intents, leases, task board, topic channels, read-set invalidation
├── trellis-context     # context_pack: retrieval, ranking, budget trimming
├── trellis-validate    # validator pipeline + LLM validator trait
├── trellis-policy      # Cedar ACLs + approval rules
├── trellis-server      # axum: HTTP + SSE; composition root
├── trellis-mcp         # rmcp server (wraps the HTTP API or calls services directly)
├── trellis-cli         # `tl`
├── trellis-git         # Git export bridge
└── trellis-sim         # deterministic simulation: N virtual agents + fault injection
```

Enforce the layering the way Mega/ceres does: the transport layer (axum/MCP) must not reach into storage directly; the domain layer doesn't depend on any web framework; enforce it with CI checks.

---

## 9. Roadmap (each phase has acceptance criteria)

| Phase | Content | Acceptance criteria |
|---|---|---|
| **M0 Core model** | `core` + `store` (SQLite + local FS) + a minimal `tl` (session/write/land, fast-forward only) | proptest: hashes of the canonical encoding are stable; the "full tree at changeset X" computed by replaying changesets is identical to a naive implementation |
| **M1 Concurrent landing** | Server-side rebase + text/structured merge drivers + CAS + receipts/idempotency + first-class conflicts | `trellis-sim`: 200 virtual agents writing concurrently (with random overlap) for 10 minutes; **zero lost acknowledged writes**, every conflict reproducible; land p99 < 100 ms (single node) |
| **M2 Coordination + MCP** | Intents/leases/task board/topic channels/read-set invalidation + `trellis-mcp` | Claude Code connects to the MCP server and two agents complete the same task without silent overwrites; lease expiry recovers |
| **M3 Context + derived data** | Derived data framework (path history, tree index, entity index, summaries) + `context_pack` + CanonYaml schema | On a 100K-word sample, `context_pack` uses ≥50% fewer tokens than "stuff in the full summary"; the continuity validator recall/precision baseline is established |
| **M4 Novel demo** | Showrunner + 20 → 200 agents writing a medium-length novel (Agent SDK orchestration script) | Compared with the baseline (Git worktree + shared folder): fewer continuity errors per 10K words, fewer tokens per accepted word, lower stale-write rate (see §10) |
| **M5 Productization** | Postgres; ACL/approval; Git export; web UI (read-only browsing + findings dashboard); cache tier (same RPCs) | Multi-tenant; usable by external pilot users |

> Do M0–M1 first and get the "correctness + concurrency" foundation right (Scion: "among the most well-tested … codebases"), then add the more interesting features.

---

## 10. Evaluation metrics

- **Correctness**: number of acknowledged writes lost (must be 0); reproducibility of conflicts.
- **Concurrency**: land throughput (/s), p50/p99 latency, CAS retry rate, conflict rate (text / structured).
- **Wasted work**: stale-read rate (the share of lands whose read set was changed by someone else), share of rewrites caused by stale facts.
- **Consistency**: continuity errors per 10K words (LLM judge + human spot check), obligation (foreshadowing) resolution rate, time to resolve findings.
- **Cost**: input/output tokens per 1,000 accepted words, prompt-cache hit rate.
- **Agent behavior**: abandonment rate (stub-and-exit), lease contention, unread inbox backlog.

---

## 11. Risks and open questions

| Risk | Mitigation |
|---|---|
| False positives / false negatives from the LLM continuity validator | Start with a findings + human spot-check calibration set; tiered severity; only auto-revert on high-confidence findings |
| Over-engineering (building infrastructure for "scale" too early) | Single node + SQLite until M4; add Postgres / cache tiers only after measuring |
| Agents don't follow the protocol (don't declare intents, write outside their scope) | Enforce in the state layer (ACLs + policy at landing), not with prompts |
| Path-level granularity is too coarse/fine for some domains | Split prose by scene; split canon by entity; the merge driver does block-level merging |
| Orchestration and substrate scopes get mixed together | This product is the **substrate**; orchestration is only a demo script (Oak removed its orchestration layer as a cautionary example) |

**Decisions that need the user's input (affect M0–M2):**

1. **Git compatibility**: is a one-way export enough (recommended), or is two-way sync required?
2. **Human participation**: do humans need to co-edit in real time with agents (this decides whether a CRDT mode becomes a priority)?
3. **Deployment shape**: self-hosted single binary first, or multi-tenant SaaS first?
4. **Orchestration boundary**: does the product include agent scheduling (spawning/budgets/roles), or only provide the substrate plus SDK examples?
5. **First vertical**: novels (as here), or code / research reports?

---

## 12. Sources

See [`references/README.md`](../../references/README.md) for local copies. Main links:

- Anthropic, *Redesigning source control at Anthropic* (2026-10-13): https://www.anthropic.com/engineering/redesigning-source-control
- Simon Willison's commentary: https://simonwillison.net/2026/Oct/13/redesigning-source-control-at-anthropic/
- The Register: https://www.theregister.com/devops/2026/10/13/anthropic-reinvents-source-control-as-claude-floods-the-world-with-code/5302943
- sunshowers, *Anthropic's Scion is heavily inspired by Mononoke*: https://sunshowers.io/posts/anthropic-mononoke/
- HackerNoon, Scion vs Cursor Origin: https://hackernoon.com/ai-agents-are-killing-the-git-client-not-git-what-anthropics-scion-and-cursors-origin-show
- Mega: https://github.com/gitmono-dev/mega · Libra: https://github.com/web3infra-foundation/libra · ScorpioFS: https://github.com/gitmono-dev/scorpiofs
- Mononoke: https://github.com/facebook/sapling/tree/main/eden/mononoke
- Oak: https://oakvcs.com/ · https://oak.space/blog
- MCP Agent Mail: https://github.com/Dicklesworthstone/mcp_agent_mail_rust
- Loro: https://loro.dev/blog/v1.0 · Jujutsu: https://github.com/jj-vcs/jj
- Papers: AgentRoom 2608.23740 · CoAgent 2606.15376 · ATM 2607.00041 · DeLM 2606.10662 · NstAgent 2609.35759 · Agentsensus 2609.32297 · Magnet/Atlas 2607.00918 · ConWriter 2608.05169 · StoryWriter 2506.16445 · Agents' Room 2410.02603 · AgentCoord 2404.11943 · Code as Agent Harness 2605.18747
- Piper: Potvin & Levenberg, CACM 2016, https://dl.acm.org/doi/10.1145/2854146

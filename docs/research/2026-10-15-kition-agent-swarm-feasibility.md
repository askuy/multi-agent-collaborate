# Feasibility assessment: multi-agent infrastructure for Kition

> Date: 2026-10-15 · Status: **reference only, not adopted for now.** The product will be implemented standalone; see [the standalone plan](../plan/2026-10-15-trellis-standalone-plan.md). This document is kept as a "real application's needs" sample and as background for integrating with Kition later
> Prerequisite reading: [the multi-agent collaboration layer research report](2026-10-15-multi-agent-collaboration-substrate.md) (codename Trellis below)
> Repositories read: `KitionAI/kition` (main, c63b039b), `KitionAI/kition-sdk` (including the Kit Standard 2.0 draft), `KitionAI/kition-film` (FilmCraft 0.5.0)

---

## 0. Conclusions

| Scenario | Feasible? | Estimated wall-clock time | Prerequisite |
|---|---|---|---|
| **20 agents write a 200-chapter novel** | **Yes** | **About 1–2 hours** (including planning and continuity repair; assumptions in §2.4) | Add an orchestration layer, a coordination layer, and batch review. The storage/merge foundation (KDM + Kitable) is mostly in place |
| **20 agents edit one video** | **Partly** | Bottleneck is media analysis and rendering/export, **not the LLM** | Feasible as "parallel analysis + per-segment rough cuts + one assembler." **Not feasible** (and not worth it) as "20 agents editing one timeline at the same time" |

Three key judgments:

1. **Kition's foundation is better than expected.** KDM (Loro CRDT, typed schema, transactions with origin, per-author undo, `export_changes/import_changes`) already solves "merging concurrent edits to one document." Kitable is a typed table on KDM with post-merge repair, which makes it a natural store for a story bible. Agent sessions are locked per session, so **different sessions can already run in parallel**. Kit tools are already exposed over MCP.
2. **What's missing isn't storage, it's coordination.** Today there is no subagent/team run, no atomic task claiming or leases, no cross-agent context packs, and no review interface for a batch of changes across 200 files. These are exactly the M1–M3 pieces of the earlier Trellis report.
3. **The secret to a fast novel is "chapter contracts," not more agents.** Chapter 37 depends on how Chapter 36 ends. If chapters are written one after another, 20 agents are no faster than 1. **Write each chapter's entry state, exit state, required events, and foreshadowing to plant or pay off into a contract during planning.** Then all 200 chapters can be written in parallel against their contracts, followed by one round of continuity repair. It's the same idea as parallel software development against agreed interfaces.

---

## 1. What Kition already has that matters for multiple agents

| Capability | Where | Use for multiple agents |
|---|---|---|
| **KDM** (Kit Data Model) | `crates/kition-kdm` (copied from kition-sdk), over `loro =1.16.2` | Every Markdown file is a KDM replica: concurrent edits merge safely, transactions carry `Origin{kind: agent, principal, label}`, undo is per author. **This is the merge engine for documents written by many agents** |
| Kit Standard 2.0 draft | `kition-sdk/docs/standard-2.md` | M10 atomic transactions, M14 change exchange, **A12 agent draft mode** (the agent edits a separate replica and a person approves the merge), and a reserved `KitCollab` capability set. **This is effectively Trellis's "session overlay + land," already in the standard** |
| **Kitable** (typed tables on KDM) | `crates/kition-kitable` (`kdm/model.rs`, `repair.rs`, `project.rs`) | Typed fields, links that merge as sets, post-merge invariant repair (renames on conflicting field names, cycle detection). **Use for the story bible: characters, timeline, foreshadowing, tasks** |
| Live document bridge + review | `kition-kits/src/document_bridge.rs`, `kition-agent/src/tools/live_documents.rs`, `DocumentAgentReview` | If a file is open in a window, an agent edit goes into it live and is highlighted for review; otherwise it falls back to an atomic file write |
| Agent harness | `crates/kition-harness`: append-only session log, step driver, tool policy pipeline, DSH (DeepSeek Harness) contracts | Each subagent can be one harness session with its own log and its own origin |
| Parallel sessions | `kition-agent/src/run.rs` `SessionLocks`: **locked per session_id** | Turns from different sessions can already run at the same time |
| Kit tools / MCP | `kition-kits/src/tools.rs`, `mcp.rs`: `POST /v1/mcp/kits`, stdio `kition-kits-mcp` | `film_inspect / film_commands / film_run` are generated from the kit's command registry |
| Kit Protocol | `docs/kit-protocol.md`: the kit is the server, **0..n `agent` clients** may connect, `commands.execute` is the only way to change a document, `view.describe/screenshot` | A video project can be driven by several agents through one kit process (command execution is serialized) |
| Workflow | `crates/kition-workflow` | Triggers + actions; can host the post-merge validation pipeline |
| Image concurrency | `kition-ai` `MAX_CONCURRENT_IMAGE_REQUESTS = 5` | The only existing "concurrent model call" mechanism |
| FilmCraft | `kition-film`: Premiere-like command registry, nested sequences, captions, Whisper speech recognition (`whisper` feature off by default), H.264/ProRes export | The video scenario's editing engine |

**Missing (relevant to this goal):**

- No subagent / team concept: no code for parallel subagents, fan-out, or merging results.
- No atomic coordination primitives (task claiming, leases, intent declarations). Fields in a KDM table are last-writer-wins, so **two agents claiming the same task at the same moment would both "succeed."** Claiming has to happen on a single authority in the runtime, not through a CRDT.
- No cross-file "batch" concept: a run touches 200 files, but review, rollback, and history are all per document.
- No context pack: the agent's context is built per turn; there's no tool that pulls the relevant set of story-bible facts and summaries.
- Kit Standard 2.0's engine contract 2 (kit engines on KDM) **is not implemented**, so the film kit's projects are not KDM replicas and can't merge drafts.
- The DSH event list is fixed (`kition-harness/src/lib.rs` tests reject "invented events"). **Subagent events must either map onto existing events (`agent/inbox/spliced`, `todo/write`, `deliverables/presented`) or be added upstream in DSH/kition-sdk first.**

---

## 2. Scenario A: 20 agents write a 200-chapter novel

### 2.1 Why "chapter contracts"

Without contracts, a writer can only start Chapter n+1 after Chapter n is done, so wall-clock time ≈ 200 × time per chapter. Adding agents doesn't help. With contracts:

```yaml
# outline/contracts/ch037.yaml  (a Kitable row is also fine)
id: ch037
arc: 4
pov: alice
beats: [arrives at the harbor, deals with the smuggler, discovers the map is fake]
entry_state:                     # world/character state at the start of this chapter (from the plan)
  alice: {location: road to the harbor, injuries: [left arm], knows: [map location]}
exit_state:                      # what must be true at the end of this chapter (the next chapter's entry_state)
  alice: {location: harbor, knows: [map location, map is fake]}
must_plant:  [ob-112: the smuggler's tattoo]      # foreshadowing to plant
must_resolve: [ob-087: whose letter it was]       # foreshadowing to pay off
word_target: 4000
neighbors: [ch036, ch038]       # only exchange summaries with these chapters
```

- Every chapter's `entry_state == previous chapter's exit_state` is **checked once in the planning stage** (a program check plus an LLM spot check).
- Writers write against the contract and **don't wait for neighboring chapters to finish**.
- Once all chapters are done, run **one continuity pass**: compare the actual text against the contract and the story bible, and generate repair tasks.

This is NstAgent's structured state (characters / past events / future obligations) moved from "accumulated while writing" to "planned first, checked afterwards."

### 2.2 Pipeline (5 stages; ⇉ marks parallel stages)

```text
S0 Setup (1 agent)              : style guide, core of the world bible, overall outline (about 10 arcs)
S1 Arc expansion ⇉ (10 agents)  : each arc → about 20 chapter contracts + story-bible additions
S1.5 Contract check (program + 1–2 agents): entry/exit continuity, every foreshadowing item paid off, no character "in two places at once"
S2 Writing ⇉ (20 agents)        : take contracts from the task board → context_pack → write → self-check → land (one file each, almost no conflicts)
S3 Continuity review ⇉ (20 agents): split by character/arc ("superfans") → findings → repair tasks
S4 Repair ⇉ + unified polish ⇉ (20 agents): handle findings; unify tone across chapters (by arc)
```

### 2.3 Mapping onto Kition

| Concept | Implementation in Kition |
|---|---|
| Prose | `chapters/ch037.md` (Markdown on KDM; one writer per chapter, while people/editors can edit at the same time and it merges safely) |
| Story bible | `bible/characters.kitable`, `bible/timeline.kitable`, `bible/obligations.kitable` (typed; agents write through transactions; Kitable repair handles concurrency) |
| Chapter contracts | `outline/contracts.kitable` (one row per chapter) |
| Task board | **An atomic queue in the runtime** (claim = compare-and-swap (CAS) in SQLite) + read-only mirrored into `tasks.kitable` for people to see |
| Subagent | One harness session = one agent; origin `agent`, principal = the person who started the run, label = role/task |
| Land | Markdown: the agent writes into its own draft replica (A12) → Trellis lands it in one go; story bible: field-level transactions |
| Validators | Contract compliance + story-bible consistency (LLM) + word count/style (program), triggered by workflow or the orchestrator |
| Review | **Batch review for the whole run**: list by chapter/finding, accept/reject per chapter or per change (extending `DocumentRevisionReview`) |

### 2.4 Time and token estimates (all assumptions labeled; must be measured)

Assumptions: about 4,000 characters of Chinese per chapter ≈ **about 5,000 output tokens** (the Chinese-to-token ratio depends on the model's tokenizer); one output stream runs at **about 50–80 tokens/s**; 20 concurrent streams don't hit the account's rate limits.

| Stage | Work | Parallelism | Estimated wall-clock time |
|---|---|---|---|
| S0 | Outline + style + bible core | 1 | 5–10 min |
| S1 + S1.5 | 200 contracts + bible additions + checks | 10 | 10–15 min |
| S2 | 200 chapters × (writing ~1–2 min + self-check/story-bible update ~1 min) | 20 → 10 chapters each | **30–45 min** |
| S3 | Continuity review (reads 1M words, in pieces) | 20 | 10–15 min |
| S4 | Repairs (assume 20–30% of chapters) + polish | 20 | 10–20 min |
| **Total** | | | **About 65–105 min** |

- Output tokens ≈ 200 × 5k × ~1.5 (including repairs/rewrites) ≈ **1.5M**.
- Input tokens ≈ 200 chapters × ~3 calls × ~20k context ≈ **12M**. Most of this is a stable prefix (style + bible core + arc outline), so **prompt-cache hit rate determines cost**: context_pack must put the stable parts first.
- Cost = output token price × 1.5M + (cached input price × cache-hit share + uncached input price × the rest) × 12M. Use the current price list of the model you choose.
- **Real constraints**: provider concurrency/TPM limits (20 parallel streams × 5k tokens/2 min ≈ 50k output TPM); long-tail tasks (the slowest chapter decides S2's finish time, so use dynamic task claiming instead of static assignment).

### 2.5 Quality risks

| Risk | Mitigation |
|---|---|
| Contracts are too rigid, so the prose reads like a fill-in-the-blanks exercise | Contracts only lock states and key events; each chapter has room for "free beats"; S4 polish is done per arc |
| Tone drifts between chapters | Shared style guide + 3 sample paragraphs as a stable prefix; S4 unifies by arc |
| Continuity checks produce false positives/negatives | Findings come with location and evidence; only high-confidence ones become automatic tasks; people spot-check |
| A story-bible change ripples out | Story-bible changes need the canon-keeper role (exclusive lease); after landing, notify sessions that read that field (read-set invalidation) |

---

## 3. Scenario B: 20 agents edit one video

### 3.1 Why "20 agents on one timeline at the same time" doesn't work

- **The film project is not on KDM** (engine contract 2 isn't implemented), so there's no draft replica and nothing to merge. Concurrency can only rely on the kit process executing commands one at a time.
- Serializing by itself is fine: a command takes milliseconds while an agent thinks for seconds (the same as ATM's "neutral steward" applying all writes). **The problem is semantic conflicts.** A ripple trim by A shifts every clip after it on the timeline, and B's `timeline.place` lands in the wrong spot by seconds. Two agents grading color on the same clip overwrite each other.
- `commands.execute` has **no precondition** (such as an expected revision or region), so an agent can't tell that "the timeline I saw has already changed."
- Agents "see" video only through `view.screenshot` and `view.describe`. Understanding footage requires frame sampling + a vision model + speech recognition (Whisper is off by default). **Compute and cost are concentrated in media understanding, not in editing commands.**
- Rendering/export mostly runs on the CPU (FilmCraft's ROADMAP rates performance at about 35–40%). 20 headless film processes decoding 4K at once would overwhelm one machine.

### 3.2 A workable pattern: map → per-segment rough cuts → reduce

```text
V0 Ingest and analysis ⇉ (process-parallel, mostly not LLM): shot detection, Whisper transcription (with word timestamps), keyframe sampling → visual descriptions (vision model)
                                → write to media_index.kitable (shared derived data, computed once, reused by every agent)
V1 Edit plan (1 agent)          : script/structure → segment contracts (duration, material scope, start/end frames, music cue points, subtitle style)
V2 Per-segment rough cuts ⇉ (N agents): each agent edits only its own **nested sequence** (Seq_07)
                                → leases are per sequence; agents never touch the same timeline
V3 Specialist passes ⇉          : subtitles (from transcription), color (by sequence), sound (loudness normalization), graphics/titles
                                → each specialist role leases its own track type or sequence
V4 Assembly (1 agent)           : place the nested sequences into the master sequence in order, add transitions
V5 Review                       : low-resolution preview render → people/agents review by segment → repair tasks → final export
```

- 20 agents can be split as: 4–6 for analysis/descriptions, 8–10 for segment rough cuts, 3–4 for specialist passes, 1 for assembly, 1–2 for review.
- Wall-clock time ≈ T(analysis, parallel by footage chunk) + T(planning) + T(slowest segment) + T(assembly) + T(preview/export render). **The LLM part is likely only a fraction; the rest depends on media length, resolution, and hardware and must be benchmarked.**

### 3.3 Changes the video scenario needs (smallest set)

1. **Kit Protocol extension: command preconditions.** `commands.execute` gets an optional `expect: { revision?, scope? }`; if `scope` (sequence/track/time range) changed since then, return `-32023 precondition_failed`. Make this change in kition-sdk first, according to the protocol versioning rules (additive, discovered through `capabilities`).
2. **Leases by sequence/track** (Trellis's coordination layer; resource names like `film:Edit.fcproj#seq/Seq_07`, `#track/A2`).
3. **Media index as derived data**: shot/transcript/description tables, reused by every agent; turn the `whisper` feature on in the agent-facing headless build.
4. **One shared headless film process per project** (multiple agent clients), instead of one process per agent.
5. Later: film on engine contract 2 (KDM), which allows true draft merging.

---

## 4. Where the "agent infrastructure" fits in Kition

### 4.1 Positioning

Trellis becomes a **set of Rust crates inside the Kition runtime** (this matches Kition's own rules: "the runtime is written in Rust," "bind to loopback only," "contracts change first, then the renderer mock and the runtime together"). Its core concepts stay domain-neutral so it can run as a cloud service later.

```text
Kition renderer (React)
  ├─ Team run panel: stages / task board / agent status / findings / batch review   ← new
  └─ Existing: documents (KDM), tables (Kitable), kit editors (KitFrame), agent chat
                │ HTTP (contracts/runtime/ updated in step)
Kition runtime (Rust, 127.0.0.1)
  ├─ kition-swarm (new) — orchestration: run → stage → task; fan-out/fan-in; budgets/concurrency/retries
  │     ├─ coord: atomic task claiming, leases (resource + TTL), intents (read/write sets), read-set invalidation
  │     ├─ context: context_pack (stable prefix first, budget trimming, record the read set)
  │     ├─ land: batch landing of draft replicas (KDM import) + run-level snapshots/rollback
  │     └─ validate: contract/consistency/program validators → findings → tasks
  ├─ kition-harness (extended) — subagent = session; parent/child relationship; event mapping within DSH contracts
  ├─ kition-agent — tools: context_pack, task_claim/complete, lease_*, land, finding_*  (also exposed over MCP)
  ├─ kition-kdm / kition-kitable — merge engine and typed story bible (already exist)
  └─ kition-kits — film/photo/... headless; command preconditions (needs a Kit Protocol extension)
```

### 4.2 How this changes the earlier Trellis report

- In the earlier report, "a CRDT is not the core consistency model" still holds. **But in Kition, KDM (CRDT) is already the merge engine for documents and tables, so it should be reused rather than replaced by a hand-written three-way merge.** Trellis's own focus shrinks to: **coordination (atomic claims/leases/intents), context, batch landing and snapshots, and validation**, which are exactly what AgentRoom says decides quality.
- Trellis's "Changeset/CAS ref" maps to "atomic import of a batch of KDM changes + a run-level version vector snapshot" in Kition. Rollback = return each document to the version vector recorded in the snapshot (KDM `checkout`/revert, respecting the M12 per-author semantics). **No new object store is needed.**
- For later multi-device or cloud collaboration, add a server-side relay plus sync protocol for KDM `export_changes/import_changes` (the reserved `KitCollab` / L6). That's the Trellis server's role in the cloud.

---

## 5. MVP and acceptance criteria

| Phase | Content | Acceptance criteria |
|---|---|---|
| **P0 Headless novel run** | `kition-swarm` (run/stage/task, atomic claims, concurrency limit, retries) + context_pack + chapter contracts + story bible on Kitable; triggered via runtime HTTP, no UI | 20 agents finish a 200-chapter (about 800k-character) draft and one continuity-repair round; **wall-clock time ≤ 2h**; zero lost writes; every chapter traceable to its contract and session log; output metrics: continuity findings per 10,000 characters, token totals, cache hit rate |
| **P1 Team run interface** | Stage/task board/agent status/findings panel + batch review (per chapter, per change) | People can see progress, pause/cancel, and accept/reject results chapter by chapter |
| **P2 Video MVP** | Media index (shots + transcription + keyframe descriptions) + per-sequence leases + command preconditions (Kit Protocol extension) + assembly agent | 10–20 agents produce a structured 5–10 min edit from 30–60 min of footage; zero misplacements caused by concurrent edits |
| **P3 Cloud/collaboration** | KDM sync relay (KitCollab), agents running in the cloud, Kition desktop as the review and control surface | Agents run in the cloud and people review on the desktop in real time |

> P0 tests the core claim (contracts + coordination give 20× parallelism without collapsing quality), so do it first. It needs no UI and no changes to the Kit Protocol.

---

## 6. Decisions you need to make

1. **Where it runs**: should P0 run inside the desktop runtime (the simplest; 20 agents are all remote API calls, so local compute isn't the bottleneck), or start directly as a separate cloud service?
2. **Subagent events and DSH**: should subagents map onto existing DSH events (does not affect conformance), or should team events be added upstream in kition-sdk/DSH first?
3. **Draft mode**: should agents write in draft replicas (A12, landed with person approval), or write directly into the workspace and rely on run-level snapshot rollback? Recommendation: write directly in P0 (it's fast), and draft mode for people in P1.
4. **The novel's language and model**: is the output Chinese? Which model is the default? (This affects the token ratio and rate-limit estimates.)
5. **Video priority**: is the novel demo enough first, or does the Kit Protocol precondition extension for the video scenario need to start in parallel?

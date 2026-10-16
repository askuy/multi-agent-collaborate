# Trellis standalone implementation plan: scenarios, implementation, testing, launch

> Date: 2026-10-15 · Status: plan draft (v0.1)
> Background: [Research report](../research/2026-10-15-multi-agent-collaboration-substrate.md) · [Kition assessment (reference only, not adopted for now)](../research/2026-10-15-kition-agent-swarm-feasibility.md)
> Decision: **build a standalone product that does not depend on Kition.** Kition only served as a reference sample for "what a real application needs." It can be integrated later through MCP/HTTP.

---

## 0. Summary

- **Showcase scenarios** (choices explained in §1):
  1. **The main demo: full-book translation with consistent terminology.** Translate a whole open-source technical book (*The Rust Programming Language* is the first candidate). The result can be scored objectively and compared with an existing community human translation, and it most clearly shows that coordination determines quality.
  2. **The most shareable demo: a playable mystery game.** About 20 agents build a branching mystery game with 300+ nodes. A program checks that it can be solved and that the clues don't contradict each other. Viewers can play it in the browser.
  3. **The headline demo: a million-character novel in about an hour.** It gets attention and directly addresses the user's original idea, but quality is subjective. It must be paired with a continuity audit and side-by-side comparisons, and it is framed as an infrastructure stress test, not a literary achievement.
  4. Video editing goes into the later roadmap, not the first batch (reasons in §1.3).
- **Implementation**: one Rust binary containing the version store, coordination layer, built-in agent runner, playbooks (scenario pipelines), MCP, CLI, and a real-time dashboard. It runs locally first (agents are remote API calls, so local compute is not the bottleneck); cloud deployment comes later.
- **Testing**: five layers. Property tests, deterministic simulation with fake agents, recorded-replay regression, real-model benchmarks against **4 baselines** with scaling curves, and human blind review. The core output is a chart showing that **without coordination, quality falls as the number of agents grows, while Trellis stays stable**.
- **Launch**: first ride the Scion news cycle with an in-depth Chinese explainer, which fills the gap noted earlier. Then publish the open-source benchmark and the three demos, each with a dashboard time-lapse and an explorer that traces every sentence back to its source.

---

## 1. Scenario selection

### 1.1 Criteria

| Criterion | Meaning |
|---|---|
| **Parallel speedup** | Does 20× parallelism produce a dramatic shortening of wall-clock time? |
| **Coordination is necessary** | If a plain map-reduce script could do it, the demo proves nothing. **The shared state must actually matter.** |
| **Objectively measurable** | Is there an automated metric (instead of "seems pretty good")? Can it be compared with a baseline? |
| **Shareable** | Can people outside the field understand it, see the result, play with it, or share it? |
| **Clean licensing** | Can the source material and outputs be published? |
| **Implementation cost** | How much scenario-specific code it needs on top of the infrastructure |

### 1.2 Candidate scores (5 = best)

| Scenario | Parallel speedup | Coordination necessary | Measurable | Shareable | Licensing | Low cost | Verdict |
|---|---|---|---|---|---|---|---|
| **B. Full-book translation + glossary** | 5 | **5** (new terms must spread to everyone in real time) | **5** (term consistency, link/build checks, comparison with human translation) | 3 | 5 (MIT/Apache) | 5 | **Main demo; do first** |
| **C. Mystery game (Ink)** | 4 | **5** (clue graph, variables, who knows what) | **5** (static analysis: solvable, reachable, no dead ends) | **5** (playable online) | 5 | 3 | **Most shareable** |
| **A. Million-character novel** | 5 | 4 (canon, chapter contracts) | 3 (continuity audit is LLM plus human judgment) | **5** (headline numbers) | 5 | 4 | **Headline; must be paired with an audit** |
| D. Fictional-world wiki (500 interlinked pages) | 5 | 4 | 4 (dead links, contradiction detection) | 3 | 5 | 4 | Optional; can be a by-product of A/C |
| E. Codebase migration / translation (with tests) | 4 | 4 | **5** (test pass rate) | 3 (crowded field) | 4 | 3 | Second phase; for a developer audience |
| H. Textbook + automatically graded exercises | 4 | 3 | 4 | 3 | 5 | 3 | Optional |
| G. 200-company research report | 5 | **2** (mostly map-reduce) | 2 | 3 | 3 | 4 | Not recommended: doesn't prove coordination |
| F. Multi-agent video editing | 3 | 4 | 2 | 5 | 3 | **1** | Later; see below |

### 1.3 Why video waits

- The cost and time go into **understanding the footage** (shot detection, speech recognition, frame descriptions) and **rendering/export**. The LLM coordination part is not the bottleneck, so the demo would show off "the video pipeline" rather than "the collaboration infrastructure."
- It needs a scriptable editing engine with command preconditions. The Kition/FilmCraft assessment showed this requires protocol changes first.
- Revisit after the infrastructure has been proven in the first three scenarios, using the "segment-parallel + single assembler" pattern.

### 1.4 Scenario designs

#### B. Full-book translation (main demo)

- **Source**: *The Rust Programming Language* (rust-lang/book, MIT/Apache-2.0, about 20 chapters, mdBook). **Comparison**: the community Chinese translation (KaiserY/trpl-zh-cn; confirm its license before publishing comparison text).
- **Why it shows coordination**: when 20 agents each translate a chapter, a term like "ownership" or "borrow checker" can easily end up with 3 different translations. Cross-chapter anchors and links can break. Style drifts.
- **Pipeline**:
  1. Term extraction (parallel by chapter) → candidate glossary.
  2. Glossary curation (1–2 agents with an exclusive lease) → `glossary.yaml` (canon).
  3. Translation by section (parallel; dynamic claiming of about 300 tasks). **When a new term appears, the agent files a term request** → it is approved → **every session that read the glossary receives an invalidation notice** → affected sections are automatically queued for re-checking.
  4. Validators: term consistency (program), code blocks untouched (program), links/anchors valid (program), `mdbook build` passes (program), fluency/accuracy (LLM judge, sampled).
  5. Repair → export a site.
- **Metrics**: term-consistency rate (expected near 100% with coordination; naive parallel should be visibly lower), number of broken links, number of build errors, blind preference against the human translation, wall-clock time, and cost.

#### C. Mystery game (most shareable)

- **Output**: an [Ink](https://github.com/inkle/ink) script (open-source narrative scripting language; inkjs can run it in the browser). About 300–500 nodes, 3–5 endings.
- **Why it shows coordination**: global variables (clues collected, characters' attitudes), the clue graph (the true culprit's evidence must be planted and reachable), and each character's knowledge boundary (who can know what). Branches written by different agents must agree on all of these.
- **Pipeline**: case design (the truth plus a list of clues: canon) → map/scene contracts (entry variables, exit variables, clues that must appear) → parallel scene writing → **static validation** (compiles; every ending reachable; the true ending's required clues are all obtainable; no undeclared variables; no dead ends) → automated playthroughs (random/search-based play-testing agents) → repair → publish.
- **Metrics**: compile/reachability/solvability (pass/fail), number of contradictions, play-tester ratings; for the audience, simply "play it."

#### A. Million-character novel (headline)

- See §2 of the Kition assessment (chapter contracts + canon + continuity audit), now implemented standalone.
- **Required pairings**: (1) the same outline run under naive parallel / single agent / Trellis, with **a published side-by-side of continuity-error counts**; (2) **source tracing**: click any paragraph to see the agent, contract, canon version, and session log that produced it; (3) full disclosure of cost and time.
- **Framing**: "An infrastructure stress test: 20 agents keeping a million characters consistent," not "AI writes a great novel."

---

## 2. Implementation plan (standalone)

### 2.1 Product shape

```text
trellis (one binary)
  ├─ server      HTTP/JSON + SSE (the ground-truth API)
  ├─ store       SQLite (WAL) + content-addressed blob directory (zstd); Postgres/S3 later
  ├─ land        changesets, server-side rebase/merge, CAS on main, idempotent receipts, first-class conflicts
  ├─ coord       atomic task claiming, leases (resource + TTL), intents (read/write sets), read-set invalidation, topic channels
  ├─ context     context_pack: stable prefix first, budget trimming, read-set tracking
  ├─ validate    validator pipeline (programs + LLM), findings → tasks
  ├─ runner      built-in agent runner (Claude Messages API, tool loop, caching, concurrency, budgets)
  ├─ playbook    YAML-defined scenario pipelines (stages/roles/prompts/validators/exporters)
  ├─ mcp         MCP server: external agents (Claude Code, etc.) can join a run as workers
  ├─ cli         `tl`
  └─ dashboard   web dashboard (SSE live: agent lanes, task board, canon changes, conflicts/findings, cost)
```

**Key decision: build our own runner instead of using only an external framework.** Demos and benchmarks need control over concurrency, budgets, retries, caching, and reproducibility. **MCP stays open at the same time**, so other agents can participate (a demo highlight: built-in workers and a Claude Code session collaborating on the same run).

### 2.2 Data model (simplified from the research report)

- `Blob` (BLAKE3, zstd) · `Changeset` (changed files only + provenance) · `Ref main` (CAS) · `Session` (base + overlay + read_set) · `Task` / `Lease` / `Intent` · `Conflict` · `Finding` · `Run` / `Stage` (a playbook execution instance, with snapshot changeset IDs for each stage).
- File types determine merge and validation: `text/markdown` (line-level three-way merge), `canon/yaml` (field-level structural merge + JSON Schema), `ink`, `binary`.
- **For the MVP, use our own three-way merge rather than a CRDT** (with a single authoritative server and landing done by the server, a CRDT isn't needed). Loro can be added later if real-time co-editing by people is required.

### 2.3 Runner (calling Claude)

- **There is no official Rust SDK**: call `POST /v1/messages` directly with `reqwest` over HTTP + SSE streaming, implementing the tool loop ourselves (stop_reason `tool_use` → execute → return all `tool_result`s in one user message). Wrap it as an internal `trellis-llm` crate with a provider trait, so other models can be plugged in later.
- **Models** (prices from the cached 2026-06 list; confirm against the official page before publishing):

  | Role | Default | Option to compare (decided by benchmark) |
  |---|---|---|
  | Planning / canon curation / review | `claude-opus-5` (thinking `adaptive`, effort `high`) | — |
  | Writing / translation workers | `claude-opus-5` (effort `medium`/`low`) | `claude-sonnet-5` |
  | Bulk extraction / summarization / LLM judging | `claude-opus-5` (effort `low`) | `claude-haiku-4-5`; non-real-time evaluation via the **Batches API (50% off)** |

  Measure "Opus 5 at lower effort" before deciding whether to bring in a multi-model mix (caches are scoped per model, so mixing models loses cache reuse).
- **Caching**: each role's system prompt + style guide + canon summary form a stable prefix, followed by `cache_control`. The dashboard shows `cache_read_input_tokens` live.
- **Concurrency and rate limits**: global semaphore + per-model token bucket; on 429, back off according to `retry-after`; long-tail tasks are claimed dynamically.
- **Budgets**: hard per-run cost cap (pause when reached); per-task token budget.
- **Tool input validation**: validate every `tool_use.input` against its schema in the runner; on failure, return a `tool_result` with `is_error: true`.

### 2.4 Playbook example (abridged translation scenario)

```yaml
playbook: translate-book
inputs: { source_repo: rust-lang/book, target_lang: zh-CN }
canon:
  - path: canon/glossary.yaml
    schema: glossary.schema.json
    write_policy: exclusive(role=terminologist)
stages:
  - id: extract_terms
    fanout: { over: "src/*.md", role: term_extractor, concurrency: 20 }
  - id: curate_glossary
    role: terminologist
  - id: translate
    fanout: { over: sections("src/*.md"), role: translator, concurrency: 20 }
    context: [canon/glossary.yaml, style/zh-CN.md, neighbors(1)]
    on_canon_change: recheck_readers        # read-set invalidation → re-check tasks
  - id: validate
    validators: [term_consistency, code_blocks_untouched, links, mdbook_build, llm_fluency(sample=0.2)]
  - id: repair
    fanout: { over: findings, role: translator }
export: [mdbook_site]
budget: { usd: 60, wall_clock: 90m }
```

### 2.5 Crate layout

```text
crates/
  trellis-core  trellis-store  trellis-land  trellis-merge  trellis-coord
  trellis-context  trellis-validate  trellis-llm  trellis-runner  trellis-playbook
  trellis-server  trellis-mcp  trellis-cli  trellis-sim  trellis-bench
web/dashboard            (Vite + React, SSE only, no extra backend)
playbooks/               translate-book.yaml  mystery-ink.yaml  novel.yaml
exporters/               mdbook  epub  ink-web
```

### 2.6 Milestones (assuming 1–2 people + Claude Code; rough estimate)

| Week | Deliverable | Gate |
|---|---|---|
| W1–2 | core/store/land/merge + `trellis-sim` | Simulation: 200 fake agents for 10 minutes, zero lost writes, all invariants hold |
| W3 | coord + server + CLI + MCP | Claude Code can claim tasks via MCP, take leases, land work, and receive invalidation notices |
| W4 | llm/runner/playbook/context | A small translation run (2 chapters × 4 agents) completes end to end; cache hit rate visible |
| W5 | **Translation demo (full book)** + 4-way baseline comparison | Metrics in §3.4 produced; publishable |
| W6 | Dashboard + source-tracing explorer | Dashboard replays in time-lapse; clicking a paragraph traces it back |
| W7 | **Mystery game demo** | All static checks pass; playable online |
| W8 | **Million-character novel** + scaling curves + benchmark repository | Within budget and time; continuity-audit comparison chart |

---

## 3. Testing plan

### 3.1 Layer 1: properties and units (on every commit)

- `proptest`: hashes of the canonical encoding are stable; replaying changesets gives the same result as a naive tree; three-way merge is commutative/idempotent where it should be; structural merge never loses unconflicted fields.
- Merge-driver regression samples (Markdown/YAML/Ink edge cases).

### 3.2 Layer 2: deterministic simulation (every commit / nightly)

- `trellis-sim`: N fake agents (scripts/random strategies, **no LLM calls**), seeded and reproducible. Fault injection: crashes while holding a lease, network delay/reordering, duplicate land requests, clock skew, CAS contention storms.
- **Invariants**: (1) an acknowledged write is never lost; (2) at any moment at most one holder per exclusive lease; (3) repeated land requests with the same idempotency key give the same result; (4) every conflict is either resolved or explicitly open; (5) invalidation notices are delivered to everyone whose read set intersects the change (no misses); (6) a task is never completed twice.
- Tools: `turmoil` or `madsim`; a failure prints the seed and can be replayed exactly.

### 3.3 Layer 3: recorded replay (every PR)

- Record LLM request/response "cassettes" from real runs; replay them in CI to cover runner/playbook/validator regressions **at zero model cost**.
- Nightly smoke test: a small real run (`claude-haiku-4-5` or Opus 5 at low effort, 2 chapters × 4 agents) checks for API drift.

### 3.4 Layer 4: benchmarks and ablations (by milestone, results published)

**Four baselines** (same inputs, same models, same total budget):

| Condition | Description |
|---|---|
| S. Single agent | One agent finishes the whole task sequentially (with memory/summaries) |
| P. Naive parallel | 20 agents in parallel, shared folder, no coordination (agents only see the outline) |
| G. Git worktrees | 20 agents in parallel worktrees, merged into main via PR/rebase; conflicts resolved by an agent |
| **T. Trellis** | Contracts + canon + leases + invalidation + validation and repair |

**Scaling curve**: N = 1, 2, 5, 10, 20, 50; record quality and wall-clock time for each condition. **Expected (to be verified)**: P's quality drops as N grows (AgentRoom's finding), while T stays flat. **This is the central chart for the launch.**

**Ablations**: remove each of chapter/scene contracts, read-set invalidation, validators, and context_pack (replaced by "stuffing in all the summaries"), to measure each component's contribution.

**Validator recall**: inject 50 known errors (term changes, character "in two places at once," broken clue chains) and measure the detection rate.

**Metrics by scenario**:

| Scenario | Objective metrics | Subjective metrics |
|---|---|---|
| Translation | Term-consistency rate, broken links, build errors, untouched code blocks | Blind preference against the human translation (20 sampled sections) |
| Mystery game | Compiles, ending reachability, solvability, undeclared variables / dead ends | Playtest ratings |
| Novel | Contract violations, continuity findings per 10,000 characters, foreshadowing payoff rate | Blind A/B (T vs P vs S, 20 sampled chapters) |
| All | Wall-clock time, total tokens, cost, cache hit rate, conflict/retry counts, stale-read rate | — |

> Evaluation runs and LLM judging all go through the Batches API to save cost. Before each benchmark run, estimate the cost and confirm the budget.

### 3.5 Layer 5: human review

- Blind A/B: reviewers don't know the condition; tasks are sampled randomly; at least 3 reviewers; agreement reported.
- Publish **all** outputs, logs, prompts, and costs so third parties can reproduce them.

---

## 4. Launch plan

### 4.1 Positioning

- **Core message**: Git was built for "humans, offline." Scion showed that agent-scale work needs something new. **Trellis brings Scion's ideas (server-side work, changesets, derived data, post-merge validation) to all agent collaboration beyond code, with coordination added.**
- **What can be proven**: (1) the scaling chart showing that coordination determines quality; (2) three verifiable demos; (3) fully reproducible results.

### 4.2 Assets

| Asset | Content |
|---|---|
| Dashboard time-lapse (60–90 s) | 20 agent lanes, the task board draining, canon changes, conflicts/invalidations lighting up and being resolved, live cost counter |
| Source-tracing explorer (online) | Read the translation/novel; click any paragraph → agent, task, contract, canon version, session log, changeset |
| Playable game | The mystery game in the browser + "the 20 agents who wrote this game" behind-the-scenes panel |
| Benchmark repository | Tasks + 4 baselines + scoring scripts, open for submissions from other frameworks |
| Chinese deep dive | "Scion in depth: why Anthropic dropped Git, and what it means for collaboration beyond code" (there is no deep Chinese article yet) |
| English blog post | "Coordination, not parallelism: what we learned running 20 agents on one book" |
| Technical write-up | arXiv-style: method + benchmark + ablations (optional) |

### 4.3 Timeline

| When | Action |
|---|---|
| **This week (riding the Scion news)** | Publish the Chinese Scion deep dive (Zhihu / WeChat public account / Juejin), with a brief preview of the project; open-source the research report and reference index |
| W5 | Translation demo + first scaling chart → Hacker News "Show HN" / X / Jike |
| W7 | Mystery game demo (playable link) → Bilibili/Xiaohongshu video + gaming/AI communities |
| W8 | Million-character novel + benchmark repository + English blog post + open-source release |

### 4.4 Risks

| Risk | Mitigation |
|---|---|
| "AI slop" backlash | Lead with translation/game (objectively verifiable); frame the novel as a stress test; disclose everything |
| Results don't match expectations (e.g., P isn't clearly worse) | Publish whatever the result is. Run the translation scenario first, where the outcome is easiest to predict and measure, as the first data point; adjust the story to the data |
| Name collision | "Trellis" already has several projects with the same name (including an AI 3D model). Check crates.io, GitHub, and domains before launch and pick a final name |
| Cost overruns | Hard budget cap per run; use the Batches API for evaluation; benchmark at small scale first |

---

## 5. Decisions needed from you

1. **First demo**: agree to translation first (fastest to prove, objective), then game, then novel?
2. **Book choice**: confirm *The Rust Programming Language* (MIT/Apache) and the English → Chinese direction?
3. **Model budget**: per-run cap (suggestion: translation ≤ $60, novel ≤ $150; estimate from actual measurements) and the overall benchmark budget.
4. **Open-source strategy**: open-source the core from the start (Apache-2.0), or open-source the benchmark and demos first and the core later?
5. **Final name**: keep the codename Trellis or pick a new name now?

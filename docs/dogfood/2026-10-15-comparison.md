# Dogfooding comparison: Trellis vs. no-Trellis baselines (2026-10-15)

> Superseded by [the four-condition comparison](2026-10-15-four-conditions.md) (one agent, Git worktrees, agents with locks, agents without locks; three rounds each, fixed Trellis). This page records the earlier single runs and how the first Trellis version lost.

All runs: `claude-sonnet-5` through headless Claude Code (`claude -p`), hard spending cap per agent, one run per condition. Harness: `dogfood/run.sh` (Trellis) and `dogfood/baseline.sh` (baselines). Raw outputs are in `dogfood/runs/` (not committed).

## Scenarios

- **textkit**: six independent functions in separate modules, with tests; every agent also adds a line under the same heading of a shared `CHANGELOG.md`.
- **pipeline**: five functions that build on each other (`normalize` ← `slugify`, `tokenize` ← `top_words` ← `summarize`), same CHANGELOG rule.

## Conditions

| Condition | How agents work |
| --- | --- |
| Trellis | Only the Trellis MCP tools (no shell, no files): task queue, sessions, landing, conflicts, notifications; a validator runs the tests after every landing |
| Solo | One agent, Claude Code's own tools (Read, Write, Edit, Bash...), runs the tests itself |
| Shared folder | Three agents in one directory at the same time, own tools; they may coordinate through a `TASKS.md` |
| Git worktrees | Tasks split round-robin; each agent on its own branch with its own tools, commits; branches merged at the end (`CHANGELOG.md merge=union`) |

## Results

| Scenario | Condition | Wall clock | Cost | Outcome |
| --- | --- | --- | --- | --- |
| textkit | Trellis, 3 agents, run 1 (line merge for CHANGELOG) | 107 s | $0.81 | all 13 tests pass; 6 conflicts (all CHANGELOG), resolved by the agents |
| textkit | Trellis, 3 agents, run 2 (`merge: union` for CHANGELOG) | 78 s | $0.49 | all pass; 0 conflicts; 74 turns; one agent idle for the first 50 s |
| textkit | **Trellis, 3 agents, run 4 (`task_next`, batched read/write, land completes the task)** | **56 s** | $0.40 | all pass; 0 conflicts; 33 turns (11 per agent); two tasks per agent |
| textkit | Solo | 65 s | $0.16 | all pass |
| textkit | Shared folder, 3 agents | 109 s | $0.63 | all pass; agents claimed tasks in TASKS.md on their own; no lost writes |
| textkit | Git worktrees, 3 agents | 59 s | $0.31 | all pass; all three branches merged cleanly |
| pipeline | Trellis, 3 agents | 86 s | $0.46 | all 5 tests pass; 0 conflicts; 4 notifications |

Total spend for these runs: about $3.6.

## Why run 2 lost, and the fix (run 4)

Run 2 was slower than one agent alone. The server was not the cause: outside model calls, each Trellis agent spent only 1.3 to 2.6 s in MCP and server calls. The time went into model turns. Each turn costs about the same in every condition (about $0.007 and 2 to 4 s). Trellis agents needed 12.3 turns per task, Git worktree agents 4.8. Two causes, both in how Trellis exposed work to agents, neither a lock:

1. **Claim herding.** Agents picked work with `task_list` and then `task_claim` on the first open task. All three listed the same queue and went for the same task; the losers got `taken` and spent another model turn to list and claim again. Session start times show it: agent-1 and agent-2 alternated through five tasks, and agent-3 opened its first and only session 50 s in. The "three agents" run was mostly two.
2. **One file per round trip.** A task took about nine single-purpose calls (claim, open, read three files, write two, land, complete), each a full model turn.

The fix (commit `bac34d1`):
- `claim_next` hands out the oldest open task atomically, so agents never race for the same one. It is exposed in the coordination layer, the server (`POST /tasks/next`), the client, `tl task next`, and the MCP tool `task_next`, which also opens the session.
- `read` takes several `paths` and `write` takes several `files`.
- `land` with a `summary` completes the session's task.

A task is now four calls: `task_next`, `read`, `write`, `land`. Run 4 on the same scenario, after the fix:
- all three agents had a task within 3.4 s of each other, and every task had started by 27 s;
- 33 turns instead of 74, 56 s instead of 78 s, $0.40 instead of $0.49;
- faster than one agent alone (65 s) and than Git worktrees (59 s).

**Cost is a different matter.** Three agents still cost more than one ($0.40 against $0.16): each agent loads its own system prompt, tool definitions, and the code, and on six short functions that fixed cost is most of the bill. Concurrency buys wall-clock time, not cheaper tokens. The cost can tip the other way on long jobs, where one agent's history grows with every task and is re-read on every turn, while each of several agents keeps a short history. None of these scenarios is long enough to show that.

## What the numbers say

1. **Concurrency pays in time only if the coordination layer costs few model turns.** Before the fix, Trellis lost to one agent (78 s vs 65 s); after it, three Trellis agents were the fastest condition (56 s). One agent alone stays the cheapest on a job this small ($0.16).
2. **Union merge mattered for Trellis**: the same scenario went from 6 conflicts, $0.81, 107 s to 0 conflicts, $0.49, 78 s.
3. **Trellis agents still have a handicap the baselines do not**: no shell and no files, so they could not run the tests and relied on the validator after landing. The baselines read and ran code locally.
4. **The shared folder did not fail here**, unlike the AgentRoom paper's uncoordinated condition: the agents improvised a claim list in TASKS.md and edited different files. With one shared file and edits by Claude Code's read-modify-write `Edit` tool, overwrites were possible but did not happen in this run.
5. **One run per condition**: these are single observations, not averages; a rerun can move any number by tens of percent. The 56 s against 59 s and 65 s are margins of that size; the difference between run 2 and run 4 in turns (74 against 33) is not.

## Where Trellis should matter, not yet tested

Many agents (≥ 6) on the same files; long tasks where one agent's context fills up; work that depends on other agents' unfinished work; shared state that must stay consistent (a story bible, an API contract); agents in separate machines or sandboxes that cannot share a folder or a Git checkout. None of the scenarios above has these properties.

## Harness issue found and fixed

The first worktrees run reported merge conflicts, but they were all in Python bytecode (`__pycache__/*.pyc`): the seed had no `.gitignore`, and the harness's final `git add -A` committed the cache that test runs had created. The agents had committed their own work correctly. Both seeds now have a `.gitignore`, the run was redone (the row above), and the invalid run is kept as `dogfood/runs/invalid-baseline-worktrees-textkit-pycache`.

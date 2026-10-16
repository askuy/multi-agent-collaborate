# One agent, Git worktrees, agents with locks, agents without locks (2026-10-15)

The same job done four ways, three rounds each. All runs: `claude-sonnet-5` through headless Claude Code (`claude -p`), $1 cap per agent, three agents where there are several. Runs go one at a time; each round runs the four conditions back to back, so they share API conditions. Harness: `dogfood/compare.sh textkit 3`, summary: `dogfood/compare.py`. Raw outputs: `dogfood/runs/cmp-textkit-r<round>-<condition>/` (not committed).

## The job

`textkit`: six independent Python functions, each in its own module with its own tests. Every task also appends one line under the same heading of a shared `CHANGELOG.md`. Six separate files plus one file that everyone touches: the shape of most real work (code plus a changelog, an index, a registry, a table of contents).

## The four conditions

| Condition | How it works |
| --- | --- |
| One agent | One Claude Code agent with its own tools (Read, Write, Edit, Bash) does all six tasks and runs the tests itself |
| Git worktrees | Tasks split round-robin up front; each agent works on its own branch in its own worktree with its own tools and commits; the branches are merged at the end (`CHANGELOG.md merge=union`) |
| Agents with locks | Three agents through Trellis. Before writing a file, an agent takes an exclusive lock on it (`intent` with `exclusive`), and keeps it until it lands. Each agent locks its own module for the whole task, and the shared CHANGELOG only for the final append. Waiting for a lock happens inside the tool call (`wait_seconds`) and costs no model turns. |
| Agents without locks | Three agents through Trellis, optimistic: each works in a private session and lands. Trellis merges at landing time; `CHANGELOG.md` uses a union merge, so concurrent appends keep every line. |

Every Trellis task is four tool calls without locks (`task_next`, `read`, `write`, `land`), seven with locks (two more `intent` calls and a separate read and write of the CHANGELOG once it is locked). Trellis agents have no shell: a validator runs the test suite after every landing.

## Results

| Condition | Wall clock, median (rounds 1, 2, 3) | Cost, median (rounds) | Model turns, median | Tests |
| --- | --- | --- | --- | --- |
| **Agents without locks** | **47 s** (43, 51, 47) | $0.30 (0.31, 0.30, 0.29) | 33 | 3/3 pass |
| Git worktrees | 84 s (57, 84, 89) | $0.31 (0.31, 0.35, 0.30) | 34 | 3/3 pass |
| One agent | 85 s (65, 89, 85) | **$0.17** (0.17, 0.19, 0.16) | 25 | 3/3 pass |
| Agents with locks | 93 s (93, 89, 273) | $0.49 (0.55, 0.47, 0.49) | 59 | 3/3 pass |

Total spend for the twelve runs: $3.86.

## What the numbers say

1. **Without locks, three agents finished in about half the time of one agent** (47 s against 85 s), and were the steadiest condition (43 to 51 s across rounds). Landings came in bursts: in rounds 2 and 3, three agents landed within 3.5 s of each other, twice per run (18.0, 21.3, 21.5 s, then 39.0, 40.5, 43.6 s in round 2). Each change merged in milliseconds at landing; nobody waited for anybody.
2. **With locks, three agents were no faster than one, and in one round three times slower.** The cause is the shared file. A lock on `CHANGELOG.md` is held for about three model turns (bring the session up to date, read and write the file, land), roughly 10 s. In ordinary software a lock is held for microseconds; for an agent, the critical section is model thinking time. Every task ends with that critical section, so the six landings came one at a time, 8 to 17 s apart (24.1, 38.2, 47.3, 55.7, 67.0, 79.3 s in round 2).
3. **Locks also pass one agent's slowness on to all of them.** In round 3 one agent's model calls took 243 s of API time for 2,679 output tokens (no API error; the service was slow for that agent). One of those slow calls happened while it held the CHANGELOG lock, and the other two agents waited inside their lock calls from 42 s to 236 s. Without locks, a slow agent only delays its own task.
4. **Git worktrees were barely faster than one agent here**, for two reasons: the split is fixed up front, so the run lasts as long as its slowest agent, with no stealing of remaining work; and the merge happens only at the end. In round 1 they were faster (57 s), in rounds 2 and 3 not.
5. **One agent is the cheapest** ($0.17 against about $0.30). Every extra agent loads its own system prompt, tool definitions, and code; on six short functions that fixed cost dominates. Concurrency buys wall-clock time, not cheaper tokens. On long jobs this can reverse, since one agent's history grows with every task and is re-read on every turn, but this job is too short to show it.
6. **Three rounds per condition.** Rounds 2 and 3 ran slower than round 1 for every condition except without-locks; API latency varies over time. The medians and the ordering are what to read, not single numbers.

## How this relates to the Scion article

Anthropic's *Redesigning Source Control* says commits at agent scale "must be cheap, incremental, and lock-free", that Scion accepts changesets with "no locking or cross-server coordination", and that they could not replicate "a global repository lock on the merge queue" and test after merging instead, letting Claude fix the conflicts that concurrency causes. The article publishes no measurements. This experiment is our own, on a small scale; its result points the same way: with agents, locks serialize work for whole model turns, while optimistic landing with merges at landing time keeps agents parallel.

## Where the setup came from

An earlier version of Trellis lost to one agent. Agents raced for the same task through `task_list` and `task_claim`, and every file read or write was its own model turn (12 turns per task against 4.8 for Git worktrees). `task_next` (atomic hand-out of the next task), batched `read`/`write`, and `land` that completes the task fixed it (commit `bac34d1`); the locks condition uses a blocking `intent` added in `e52a321`. All runs above use the fixed version.

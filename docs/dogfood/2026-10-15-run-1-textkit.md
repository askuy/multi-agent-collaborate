# Dogfooding run 1: three Claude Code agents implement `textkit` (2026-10-15)

**Setup.** `dogfood/code/run.sh 3 1 claude-sonnet-5`: three headless Claude Code agents (`claude -p`) with no built-in tools (`--tools ""`), only the Trellis MCP server (`--strict-mcp-config`, `--allowedTools mcp__trellis`), and a hard cap of $1 each. The repository was seeded through the Git bridge with a six-function Python package whose tests skip unimplemented functions; six tasks were queued; a validator ran the test suite after every landing. Every agent had to add a line under the same heading of a shared `CHANGELOG.md`.

## Results

| Metric | Value |
| --- | --- |
| Wall clock | 107 s |
| Cost | $0.81 total (agent-1 $0.27 / 30 turns, agent-2 $0.32 / 36 turns, agent-3 $0.22 / 23 turns) |
| Tasks | 6 / 6 done, two per agent, no task claimed twice |
| Landings | 6 by agents (+1 seed import), all on one first-parent chain |
| Conflicts | 6, all resolved by the agents; all in `CHANGELOG.md`, all `TextOverlap`; two sessions conflicted twice (main moved again before they re-landed) |
| Code conflicts | 0 (each agent owned different modules) |
| Validator FAILs | 0 |
| Final state | exported to Git as 6 commits authored by the agents; `python3 -m unittest`: 13 tests, all run, all pass; every agent's CHANGELOG line kept |
| Agent behavior | all three stopped on their own after checking for open tasks and validator failures |

## Findings

1. **The workflow works end to end with real agents**: claiming, private sessions, landing, conflict resolution through `conflict_show` / `conflict_resolve`, task completion, Git export. No human intervention.
2. **All conflicts came from append-style edits** to one shared file. A line-based three-way merge treats concurrent insertions at the same position as a conflict, even though keeping both is obviously right for a changelog. Next step: a *union* merge mode for append-style paths (as Git's `merge=union` attribute), configured in the policy file, which would have removed all six conflicts and the turns spent resolving them.
3. **Re-conflicts** (two sessions) happen when `main` moves between resolving and re-landing. With a union mode for the changelog this disappears here; in general it is the expected cost of optimistic landing, and agents handled it.
4. **Notifications were barely exercised**: agents read the files they owned, and nobody else changed those files. A scenario where agents depend on each other's code (for example a function built on another) is needed to measure notifications and stale reads.
5. **A real bug found while preparing the run**: with `HTTP_PROXY` set and no matching `NO_PROXY`, requests to a local server went through the proxy. Fixed in the client (`no_proxy` for loopback servers) with a test that fails without the fix.

## Not yet measured

Baselines (one agent alone; three agents on a shared folder without coordination; three agents in Git worktrees merged afterwards) on the same scenario, and larger agent counts. Each baseline run costs about the same as this one.

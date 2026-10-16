# A long job: 60 tasks, one agent against five (2026-10-16)

Does concurrency pay off once the work is long enough? Sixty tasks, five agents, two rounds per condition, `claude-sonnet-5`, $6 cap per agent. Every condition gets the same integration step (an extra agent fixes a red result); none needed it. Harness: `TRELLIS_DOGFOOD_INTEGRATE=1 dogfood/compare.sh longkit 2 5 6`. Raw outputs: `dogfood/runs/cmp-longkit-r<round>-<condition>/` (not committed).

## The job

`longkit`: sixty independent modules with business rules, each with its own tests. Examples: semantic version ordering, CSV fields, Luhn, ISBN and IBAN checks, business days, amortized loan payments, progressive tax, coupon stacking, free calendar slots, numbers in English words. Every task also appends one line to a shared `CHANGELOG.md`. Each module's tests were checked against a reference implementation kept out of the seed (`dogfood/scenarios/longkit/make.py`).

## Results

| Condition | Time to green, median (rounds) | Cost, median (rounds) | Agent turns, median | Green |
| --- | --- | --- | --- | --- |
| Git worktrees, 5 agents | **218 s** (234, 203) | **$2.07** (2.20, 1.94) | 230 | 2/2 |
| Trellis without locks, 5 agents | 334 s (334, 334) | $4.74 (4.73, 4.75) | 256 | 2/2 |
| One agent | 741 s (940, 542) | $2.19 (3.05, 1.32) | 45 | 2/2 |
| Trellis with locks, 5 agents | 1103 s (1190, 1016) | $7.86 (7.67, 8.05) | 498 | 2/2 |

Spend for these eight runs: $33.72.

## What the numbers say

1. **On a long job, concurrency without locks pays off.** Five agents in Git worktrees finished 3.4 times faster than one agent (218 s against 741 s), and five Trellis agents without locks 2.2 times faster (334 s). One agent took 9 to 16 minutes; it varied a lot between rounds (69 turns against 21), depending on how much it batched.
2. **With locks, five agents were slower than one** (1103 s against 741 s) and cost the most ($7.86). Every task ends by appending to the shared changelog under an exclusive lock, held for several model turns, so sixty such critical sections run one after another.
3. **On independent tasks, Trellis without locks was slower and costlier than Git worktrees**, though turn counts were close (about 51 against 46 per agent). The difference is the agent interface, not coordination: no conflicts, notifications, or validator failures happened, and time spent in Trellis itself is negligible. Per agent:

   | Per agent, about 12 tasks | Trellis without locks | Git worktrees |
   | --- | --- | --- |
   | Context per turn (cache reads / turns) | about 40k tokens | about 10k tokens |
   | Output tokens | about 27k | about 16k |
   | Model time | about 305 s | about 155 s |

   The Trellis agents work only through MCP tools:
   - `write` takes whole files, so each task re-sends the whole module and the whole changelog. The changelog grows to 64 lines (6 KB), so later tasks re-read and re-write all of it.
   - Every result (file contents, pretty-printed receipts) stays in the conversation.

   The worktree agents use Claude Code's own tools: `Edit` adds one changelog line without rewriting the file.
4. **What worktrees cannot do** is shown in [the interface-change scenario](2026-10-15-shop-interface-change.md): there, isolated branches merged into a red build three times out of three, and Trellis stayed green. On sixty independent tasks nothing needs to be shared, so that strength does not show.

## What to change in Trellis

The coordination layer is not the bottleneck; the tool interface is. Candidates, in the order of expected effect:

- **Append without reading.** For a path merged with `merge: union`, appending a line does not depend on the current content, so an `append` (or `write` with `append: true`) can skip reading and rewriting the changelog.
- **Edits instead of whole files.** An `edit` tool (replace one string), as Claude Code's `Edit`, for changes to existing files.
- **Short results.** A one-line landing outcome instead of the pretty-printed receipt.
- **Native tools on a Trellis checkout.** Agents keep Claude Code's Read, Edit, and Bash (and can run the tests) in a `tl open` checkout, and land with `tl land`. This gives the same interface as the worktree agents, so the comparison measures only landing continuously against merging at the end.

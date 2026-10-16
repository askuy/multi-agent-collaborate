# Final comparison: one agent, Git worktrees, Trellis with and without locks (2026-10-16)

This is the comparison the launch article reports. Every condition used the same setup, so the only difference between them is how the agents coordinate.

## Setup

- **Model and harness.** `claude-sonnet-5` through headless Claude Code (`claude -p`), `--effort medium` for every agent, three rounds per condition, medians reported.
- **Same tools for everyone.** Every agent uses Claude Code's own tools (Read, Write, Edit, Bash, Glob, Grep) and can run the tests. The Trellis agents work in a `tl` checkout:
  - `tl start . --count 4` claims tasks (one at a time in `shop`);
  - `tl update .` brings in what the others landed;
  - `tl land . --summary` lands the work and completes the tasks;
  - with locks, `tl lock <path> --wait` before writing a file.
- **Outcome measured.** Time and cost until every test passes. A result that ends red gets one repair agent, timed and billed to its condition.
- **Git worktrees.** The seed's `.gitattributes` gives `CHANGELOG.md` Git's union merge, the same rule as Trellis's policy. The clock stops when the last agent finishes. The five `git merge` commands after that take well under a second and are not counted. On `longkit` every merge was clean and the merged tests passed in all three rounds, so no repair time applies there.
- **Commands.**
  - `TRELLIS_DOGFOOD_EFFORT=medium TRELLIS_DOGFOOD_INTEGRATE=1 CONDITIONS="solo worktrees native-locked native" TRELLIS_DOGFOOD_MAX_CLAIMS=4 dogfood/compare.sh longkit 3 5 6`
  - the same for `shop` with `CONDITIONS="solo worktrees native" TRELLIS_DOGFOOD_MAX_CLAIMS=1 ... shop 3 3 2`
- **Raw outputs.** `dogfood/runs/cmp-{longkit,shop}-r<round>-<condition>/` (not committed). The numbers used in the article are in `docs/launch/assets/data.json`.
- **Spend.** $33.61 for these 27 runs, including the six `worktrees-plain` runs below.

## A long job: 60 tasks, five agents

| Condition | Time to green, median (rounds) | Cost, median (rounds) | Turns, median |
| --- | --- | --- | --- |
| Git worktrees, Git's default merge, conflict repair included | 3:52 (433, 232, 151 s) | $1.73 | 217 |
| Git worktrees, union-merged changelog | 3:14 (194, 179, 198 s) | $1.54 | 194 |
| Trellis, no locks | 3:23 (238, 201, 203 s) | $2.09 | 266 |
| Trellis, with locks | 6:24 (389, 384, 252 s) | $2.29 | 289 |
| One agent | 8:52 (529, 613, 532 s) | $1.90 | 76 |

Every run ended green with no repair.

- **Five agents without locks were 2.6 to 2.7 times faster than one agent**, whether they landed through Trellis (3:23) or merged Git branches at the end (3:14). The two are within each other's round-to-round spread.
- **Locks made the same five agents 1.9 times slower** (6:24 against 3:23), though still faster than one agent. With locks, every batch ends by appending to the shared changelog under an exclusive lock, held across several model turns. In the median round:
  - with locks, consecutive landings were never closer than 9.5 s (19.5 s on average);
  - without locks, landings came as close as 0.6 s apart (9.7 s on average).
- **Trellis puts finished work on main as it lands.** In its median round, 32 of the 60 tasks were on main and tested by the two-minute mark. With Git worktrees nothing reaches main until the branches are merged at the end.
- **On cost, Trellis without locks was about a third above Git worktrees** ($2.09 against $1.54). Each batch adds turns to claim, update, and land: 266 turns against 194.

## An interface change: one agent changes money from floats to cents while others build on it

| Condition | Time to green, median (rounds) | Cost, median | Green without a repair agent |
| --- | --- | --- | --- |
| One agent | 1:10 (70, 68, 72 s) | $0.16 | 3/3 |
| Git worktrees, Git's default merge | 3:21 (201, 321, 193 s, repair included) | $0.55 | 0/3 (a resolver agent every round) |
| Git worktrees, union-merged changelog | 1:51 (111, 107, 125 s, repair included) | $0.42 | 0/3 |
| Trellis, no locks | 2:26 (123, 146, 173 s) | $0.73 | 3/3 |

- **Git worktrees.** Every branch passed its own tests, and every merge was free of textual conflicts. Every merged result still failed 3 tests. A repair agent then needed 41 to 56 s.
- **Trellis.** Every landing was re-tested on main (`docs/launch/assets/data.json`, `shop_rounds[].main`). The landing that changed the interface turned main red, and the validator reported it at once. The agents then fixed the affected callers during the run, through `tl update` and the "Fix ..." tasks. Main was red for 45, 28, and 52 s in the three rounds, and green from then to the end.
- **One agent was fastest and cheapest here.** It changes the interface before writing the rest, and five small tasks leave little to parallelize.

## Not used in the article, and why

| Data | Why it is left out |
| --- | --- |
| Runs before `task_next` (`2026-10-15-comparison.md`, run 1 and 2) | A claim race in the Trellis API made agents idle; fixed in `bac34d1` |
| Trellis through MCP tools only (`2026-10-15-four-conditions.md`, `2026-10-15-shop-interface-change.md`, `2026-10-16-longkit-60-tasks.md`, `dogfood/runs/v1/`) | Different tools from the other conditions (whole-file writes, no local tests); replaced by the same-tools comparison above |
| `dogfood/runs/v1/cmp-longkit-r1-native` | Stopped by its spending cap at 46 of 60 tasks |
| `dogfood/runs/v2/` | No fixed effort level: one agent's thinking ranged from 1.7k to 55k tokens between runs (5 to 15 minutes for the same job) |

The earlier write-ups stay in `docs/dogfood/` as the record of how the setup was fixed.

## Git worktrees with Git's default merge (`worktrees-plain`)

The worktrees rows above first gave the shared changelog Git's union merge, which Trellis applies by policy. Most Git setups do not. So both jobs ran three more rounds with Git's default merge: `CONDITIONS=worktrees-plain`, same effort level and tools.

When a merge stopped on a conflict, one more agent resolved it (`dogfood/baseline.sh`). Its time and cost are added to the condition (`merge repair` in each report).

**60 tasks.** Agent time, conflicts, and repair time per round:

| Round | Agent time | Conflicts | Repair time | Total |
| --- | --- | --- | --- | --- |
| 1 | 246 s | 4 | 187 s | 433 s |
| 2 | 185 s | 1 | 47 s | 232 s |
| 3 | 151 s | 0 | 0 s | 151 s |

- Every conflict was in `CHANGELOG.md`. Whether two branches collide there depends on where each agent put its lines.
- The resolver agents kept every entry, and all 60 tests passed.

**Interface change.** Every round had two merges stop on changelog conflicts. Each time, the merged code still called the old money functions. The resolver agents fixed both: they kept every changelog entry and ported the callers to the new interface.
- Repair took 132, 241, and 136 s.
- The tests passed after every repair.


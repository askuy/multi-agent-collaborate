# An interface changes while others build on it: one agent, Git worktrees, Trellis (2026-10-15)

Do the agents learn about each other's work, and does it matter? In [the first comparison](2026-10-15-four-conditions.md) the tasks were independent, so nobody needed to know what anybody else did. Here they cannot work without it. Three rounds per condition, `claude-sonnet-5`, three agents where there are several, $1 cap per agent. Harness: `CONDITIONS="solo worktrees lock-free" TRELLIS_DOGFOOD_INTEGRATE=1 dogfood/compare.sh shop 3`. Raw outputs: `dogfood/runs/cmp-shop-r<round>-<condition>/` (not committed).

## The job

`shop`: money helpers for a shop. Five tasks:

1. **Move `shop.money` from floats to integer cents.** `parse_amount` and `format_amount` go away; `to_cents` and `format_cents` replace them. The task also says to update every caller.
2. to 5. Implement `add_tax`, `apply_discount`, `split_bill`, and the receipt `render`, each parsing and formatting amounts through `shop.money`.

The three agents start at the same time, so tasks 2 and 3 begin while `shop.money` still has the old interface. Code written against the old interface passes its tests until the migration lands, then fails with `AttributeError`. This was checked with reference implementations before any run. To end up green, somebody has to learn about the other side's change in time.

## The integration step

Every condition gets the same rule: if the combined result fails its tests, one more agent with Claude Code's own tools fixes it (`dogfood/integrate.sh`). Its time and cost count toward the condition. The table compares time and money until the whole suite passes.

## Results

| Condition | Time to green, median (rounds) | Cost, median (rounds) | Green without integration | Green in the end |
| --- | --- | --- | --- | --- |
| One agent | 101 s (82, 103, 101) | **$0.19** | 3/3 | 3/3 |
| Git worktrees | 161 s (135, 161, 211) | $0.51 | **0/3** | 3/3 |
| **Trellis, without locks** | **101 s** (96, 101, 108) | $0.46 | **3/3** | 3/3 |

Git worktrees: every branch passed its own tests, all branches merged without a textual conflict, and the merged result failed 3 tests every round. An integration agent then needed 57 to 102 s to repair it.

Trellis: green every round with no integration. The validator, which runs the suite after every landing, never saw a failure, so `main` was never broken at any point.

## How the Trellis agents learned about each other's work

Both ways the timing can go happened:

- **Rounds 1 and 2: the dependent code lands first.** The migrating agent read every caller at the start (they were stubs). While it worked, the others landed `add_tax`, `apply_discount`, `split_bill`, and `render` against the old interface. Each landing showed up in the migrating agent's tool results as a `[trellis]` note (in round 1: four notes, one per landing). It rebased, re-read the new callers, and moved them to the new interface in the same session. One changeset changed 6 files: `money.py`, the four callers, and the changelog. In round 2 its message reads "Migrate shop.money to integer cents (to_cents/format_cents) and update all callers (tax, discount, split, receipt)".
- **Round 3: the interface change lands first.** The agents on `add_tax` and `apply_discount` had read the old `money.py`. The note that it changed reached them before they landed, and they wrote against the new interface ("Implement shop.tax.add_tax using integer-cents API").

In Git worktrees neither can happen: each agent sees only its own branch until the merge.

## What the numbers say

1. **Seeing each other's changes is what kept `main` green.** Same agents, same tasks: three times out of three the isolated branches combined into a broken build, and three times out of three Trellis did not. This is the problem the Mega and Scion write-ups describe: agents in isolated branches lack the shared, current context.
2. **On time, Trellis tied with one agent (101 s each) and beat Git worktrees by 60 s.** The interface change sits on the critical path. When the dependent code lands first, the migrating agent has to rewrite it, which a single agent avoids by changing the interface before writing the rest. Five short tasks leave the three agents little to gain in parallel.
3. **One agent is still the cheapest.** Each extra agent loads its own prompt, tools, and code.
4. **Three rounds per condition**: single observations vary by tens of percent; read the medians and the green columns.

#!/usr/bin/env python3
"""Summarize dogfood/compare.sh runs: per condition, every round and the median."""

import json
import pathlib
import re
import statistics
import sys

CONDITIONS = ["solo", "worktrees", "worktrees-plain", "locked", "lock-free", "native-locked", "native"]


def run_stats(run: pathlib.Path) -> dict | None:
    report = run / "report.md"
    if not report.exists():
        return None
    text = report.read_text()
    wall = int(re.search(r"wall clock: (\d+)s", text).group(1))
    results = [json.loads(p.read_text()) for p in sorted(run.glob("agent-*/result.json"))]
    # Green means every test ran and passed: a test still skipped at the end is a stub nobody
    # implemented (for example an agent stopped by its spending cap).
    passed = bool(re.search(r"(final tests|final test run on the exported Git repository): OK$", text, re.M))
    cost = sum(r.get("total_cost_usd") or 0 for r in results)
    merge_repair = re.search(r"merge repair: (\d+)s; cost: \$([\d.]+)", text)
    if merge_repair:
        wall += int(merge_repair.group(1))
        cost += float(merge_repair.group(2))
    integration = re.search(r"integration: (\d+)s; cost: \$([\d.]+); tests after: (.*)", text)
    green = passed
    if integration:
        wall += int(integration.group(1))
        cost += float(integration.group(2))
        green = integration.group(3).strip() == "OK"
    return {
        "wall_s": wall,
        "cost": cost,
        "turns": sum(r.get("num_turns") or 0 for r in results),
        "passed": passed,
        "integrated": bool(integration),
        "green": green,
    }


def main() -> None:
    scenario = sys.argv[1]
    runs = pathlib.Path(__file__).parent / "runs"
    rows = []
    print("| Condition | Rounds | Time to green, median (each) | Cost, median (each) | Agent turns, median | Green without integration | Green in the end |")
    print("| --- | --- | --- | --- | --- | --- | --- |")
    for condition in CONDITIONS:
        dirs = [d for d in sorted(runs.glob(f"cmp-{scenario}-r*-{condition}"))
                if re.fullmatch(rf"cmp-{re.escape(scenario)}-r\d+-{re.escape(condition)}", d.name)]
        stats = [s for s in (run_stats(d) for d in dirs) if s]
        if not stats:
            continue
        walls = [s["wall_s"] for s in stats]
        costs = [s["cost"] for s in stats]
        print(
            f"| {condition} | {len(stats)} | {statistics.median(walls):.0f} s ({', '.join(map(str, walls))}) "
            f"| ${statistics.median(costs):.2f} ({', '.join(f'{c:.2f}' for c in costs)}) "
            f"| {statistics.median(s['turns'] for s in stats):.0f} "
            f"| {sum(s['passed'] for s in stats)}/{len(stats)} | {sum(s['green'] for s in stats)}/{len(stats)} |"
        )
        rows.append({"condition": condition, "runs": stats})
    (runs / f"cmp-{scenario}.json").write_text(json.dumps(rows, indent=2))


if __name__ == "__main__":
    main()

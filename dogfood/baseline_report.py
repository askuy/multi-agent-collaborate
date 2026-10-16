#!/usr/bin/env python3
"""Summarize a baseline run (no Trellis): cost per agent and the state of the final tree."""

import json
import pathlib
import re
import sys


def main() -> None:
    run, final, mode, elapsed, tests, merges = (
        pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3], int(sys.argv[4]), sys.argv[5], sys.argv[6]
    )
    agents = []
    for directory in sorted(p for p in run.glob("agent-*") if p.is_dir()):
        try:
            result = json.loads((directory / "result.json").read_text())
        except (OSError, ValueError):
            result = {}
        agents.append({
            "agent": directory.name,
            "cost_usd": round(result.get("total_cost_usd") or 0, 4),
            "turns": result.get("num_turns"),
            "outcome": result.get("subtype"),
        })
    changelog = (final / "CHANGELOG.md").read_text() if (final / "CHANGELOG.md").exists() else ""
    entries = [line for line in changelog.splitlines() if line.startswith("- ")]
    stubs = sorted(str(p.relative_to(final)) for p in final.rglob("*.py") if "@stub" in p.read_text() and p.name != "_stub.py")
    conflict_markers = sorted(
        str(p.relative_to(final)) for p in final.rglob("*") if p.is_file() and ".git" not in p.parts
        and re.search(r"^(<<<<<<<|>>>>>>>)", p.read_text(errors="ignore"), re.M)
    )
    report = {
        "mode": mode,
        "elapsed_seconds": elapsed,
        "agents": agents,
        "total_cost_usd": round(sum(a["cost_usd"] for a in agents), 4),
        "final_tests": tests,
        "changelog_entries": len(entries),
        "stubs_left": stubs,
        "files_with_conflict_markers": conflict_markers,
        "merges": merges.split(),
    }
    (run / "report.json").write_text(json.dumps(report, indent=2))
    print(f"## Baseline {mode}: {run.name}\n")
    print(f"- wall clock: {elapsed}s; total cost: ${report['total_cost_usd']}")
    for a in agents:
        print(f"  - {a['agent']}: ${a['cost_usd']}, {a['turns']} turns, {a['outcome']}")
    print(f"- final tests: {tests}")
    print(f"- CHANGELOG entries: {len(entries)}; stubs left: {stubs or 'none'}; conflict markers: {conflict_markers or 'none'}")
    if report["merges"]:
        print(f"- merges: {' '.join(report['merges'])}")


if __name__ == "__main__":
    main()

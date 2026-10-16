#!/usr/bin/env python3
"""Summarize a dogfooding run: cost and turns per agent (from Claude Code's JSON output) and
coordination metrics (from the Trellis server). Writes report.json and prints Markdown."""

import collections
import json
import pathlib
import sys
import urllib.request


def main() -> None:
    run, server, repo, elapsed, tests = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5]

    # The server is local: never send these requests through an HTTP proxy from the environment.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def get(path: str):
        with opener.open(f"{server}/repos/{repo}{path}") as response:
            return json.load(response)

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
            "error": result.get("is_error"),
        })

    changesets = get("/changesets?after_seq=0")
    conflicts = get("/conflicts?open=false")
    tasks = get("/tasks")
    log = get("/log")
    sessions = get("/sessions?open=false")
    validator_fails = [e for e in log if "validator" in e["topics"] and e["kind"] == "fail"]
    report = {
        "elapsed_seconds": elapsed,
        "agents": agents,
        "total_cost_usd": round(sum(a["cost_usd"] for a in agents), 4),
        "landings": len(changesets),
        "landings_by_principal": collections.Counter(c["changeset"]["provenance"]["principal"] for c in changesets),
        "conflicts": len(conflicts),
        "conflicts_resolved": sum(1 for c in conflicts if c["status"] == "resolved"),
        "tasks": collections.Counter(t["status"]["state"] for t in tasks),
        "sessions": collections.Counter(s["status"]["state"] for s in sessions),
        "notifications": sum(len(get(f"/sessions/{s['id']}/notifications?all=true")) for s in sessions),
        "unacked_notifications": sum(len(get(f"/sessions/{s['id']}/notifications")) for s in sessions),
        "reads_per_session": [len(s["reads"]) for s in sessions],
        "log_entries": len(log),
        "validator_fails": len(validator_fails),
        "validator_fails_open": sum(1 for e in validator_fails if e["status"] == "open"),
        "final_tests": tests,
    }
    (run / "report.json").write_text(json.dumps(report, indent=2))
    print(f"## Dogfooding run {run.name}\n")
    print(f"- wall clock: {elapsed}s; total cost: ${report['total_cost_usd']}")
    for a in agents:
        print(f"  - {a['agent']}: ${a['cost_usd']}, {a['turns']} turns, {a['outcome']}")
    print(f"- landings: {report['landings']} {dict(report['landings_by_principal'])}")
    print(f"- conflicts: {report['conflicts']} ({report['conflicts_resolved']} resolved)")
    print(f"- tasks: {dict(report['tasks'])}; sessions: {dict(report['sessions'])}")
    print(f"- validator FAILs: {report['validator_fails']} ({report['validator_fails_open']} still open); log entries: {report['log_entries']}")
    print(f"- notifications: {report['notifications']} ({report['unacked_notifications']} never acknowledged)")
    print(f"- final test run on the exported Git repository: {tests}")


if __name__ == "__main__":
    main()

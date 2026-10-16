#!/usr/bin/env bash
# Runs the four conditions on one scenario, round after round, one run at a time:
# solo (one agent), Git worktrees, Trellis with locks, Trellis without locks; `native` (Trellis
# without locks, agents with Claude Code's own tools on a checkout) on request through CONDITIONS.
#
#   dogfood/compare.sh <scenario> [rounds=3] [agents=3] [budget=1] [model=claude-sonnet-5]
#
# Runs land in dogfood/runs/cmp-<scenario>-r<round>-<condition>; dogfood/compare.py summarizes.
# CONDITIONS="solo worktrees lock-free" picks a subset. TRELLIS_DOGFOOD_INTEGRATE=1 adds the
# integration step (dogfood/integrate.sh) to every condition.
# TRELLIS_DOGFOOD_EFFORT (low, medium, high, ...) fixes the model's effort level for every agent,
# so how much a model chooses to think does not differ between conditions.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCENARIO="${1:?usage: dogfood/compare.sh <scenario> [rounds] [agents] [budget] [model]}"
ROUNDS="${2:-3}"
AGENTS="${3:-3}"
BUDGET="${4:-1}"
MODEL="${5:-claude-sonnet-5}"

for round in $(seq 1 "$ROUNDS"); do
  for condition in ${CONDITIONS:-solo worktrees locked lock-free}; do
    out="$ROOT/dogfood/runs/cmp-$SCENARIO-r$round-$condition"
    [ -f "$out/report.md" ] && continue  # already done: resume after an interruption
    rm -rf "$out"
    echo "== round $round: $condition"
    case "$condition" in
      solo) TRELLIS_DOGFOOD_DIR="$out" "$ROOT/dogfood/baseline.sh" solo "$SCENARIO" 1 "$BUDGET" "$MODEL" ;;
      worktrees) TRELLIS_DOGFOOD_DIR="$out" "$ROOT/dogfood/baseline.sh" worktrees "$SCENARIO" "$AGENTS" "$BUDGET" "$MODEL" ;;
      worktrees-plain) TRELLIS_DOGFOOD_DIR="$out" "$ROOT/dogfood/baseline.sh" worktrees-plain "$SCENARIO" "$AGENTS" "$BUDGET" "$MODEL" ;;
      locked) TRELLIS_DOGFOOD_DIR="$out" "$ROOT/dogfood/run.sh" "$SCENARIO" "$AGENTS" "$BUDGET" "$MODEL" locked ;;
      lock-free) TRELLIS_DOGFOOD_DIR="$out" "$ROOT/dogfood/run.sh" "$SCENARIO" "$AGENTS" "$BUDGET" "$MODEL" ;;
      native) TRELLIS_DOGFOOD_DIR="$out" "$ROOT/dogfood/run.sh" "$SCENARIO" "$AGENTS" "$BUDGET" "$MODEL" native ;;
      native-locked) TRELLIS_DOGFOOD_DIR="$out" "$ROOT/dogfood/run.sh" "$SCENARIO" "$AGENTS" "$BUDGET" "$MODEL" native-locked ;;
    esac | grep -E "wall clock|tests|test run|integration"
  done
done
python3 "$ROOT/dogfood/compare.py" "$SCENARIO"

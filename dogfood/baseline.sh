#!/usr/bin/env bash
# Baselines without Trellis, for comparison with dogfood/run.sh on the same scenario. Agents use
# Claude Code's own tools (Read, Write, Edit, Bash, Glob, Grep) and can run the tests themselves.
#
#   dogfood/baseline.sh <solo|shared|worktrees> <scenario> [agents=3] [budget_usd_per_agent=1] [model]
#
#   solo       one agent does every task
#   shared     all agents work in one directory at once (they may coordinate through TASKS.md)
#   worktrees  tasks split round-robin; each agent commits on its own Git branch; the branches
#              are merged at the end (CHANGELOG.md uses Git's merge=union, as Trellis's policy does)
#   worktrees-plain  the same with Git's default merge. With TRELLIS_DOGFOOD_INTEGRATE set, a merge
#              that conflicts is handed to one more agent to resolve, timed and billed as repair
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE="${1:?usage: dogfood/baseline.sh <solo|shared|worktrees> <scenario> [agents] [budget] [model]}"
SCENARIO="${2:?scenario}"
AGENTS="${3:-3}"
BUDGET="${4:-1}"
MODEL="${5:-claude-sonnet-5}"
[ "$MODE" = solo ] && AGENTS=1
DIR_S="$ROOT/dogfood/scenarios/$SCENARIO"
NAME_RUN="baseline-$MODE-$SCENARIO-$(date +%Y%m%d-%H%M%S)"
RUN="${TRELLIS_DOGFOOD_DIR:-$ROOT/dogfood/runs/$NAME_RUN}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/trellis-$NAME_RUN.XXXX")"
TOOLS="Read,Write,Edit,Bash,Glob,Grep"
mkdir -p "$RUN"

git_c() { git -c user.name=baseline -c user.email=baseline@example.com "$@"; }

# The repository every mode starts from.
cp -R "$DIR_S/seed" "$WORK/main"
[ "$MODE" = worktrees ] && echo "CHANGELOG.md merge=union" >"$WORK/main/.gitattributes"
if [ "$MODE" = shared ]; then
  { echo "# Tasks (write your name next to a task before you start it)"; echo
    cut -f1 "$DIR_S/tasks.tsv" | sed 's/^/- [ ] /'; } >"$WORK/main/TASKS.md"
fi
git -C "$WORK/main" init -q
git -C "$WORK/main" add -A
git_c -C "$WORK/main" commit -qm "$SCENARIO skeleton"

task_list() { # agent index (1-based) -> its tasks as a bulleted list
  local i="$1" n=0
  while IFS=$'\t' read -r subject detail _scope; do
    [ -z "$subject" ] && continue
    if [[ "$MODE" != worktrees* ]] || [ $(( n % AGENTS + 1 )) -eq "$i" ]; then
      printf -- '- %s: %s\n' "$subject" "$detail"
    fi
    n=$((n + 1))
  done <"$DIR_S/tasks.tsv"
}

START=$(date +%s)
PIDS=()
for i in $(seq 1 "$AGENTS"); do
  NAME="agent-$i"
  case "$MODE" in
    solo) CWD="$WORK/main"; MODE_TEXT="You are working alone on the Python project in this directory."; FINISH="" ;;
    shared) CWD="$WORK/main"; MODE_TEXT="$AGENTS agents (agent-1 to agent-$AGENTS) work in this same directory at the same time, on the same task list. Coordinate through files if you want, for example by writing your name next to a task in TASKS.md before you start it."; FINISH="" ;;
    worktrees|worktrees-plain) CWD="$WORK/$NAME"; git -C "$WORK/main" worktree add -q -b "$NAME" "$CWD"
      MODE_TEXT="You work on your own Git branch; other agents work on other tasks on other branches, merged later."
      FINISH="When your tasks are done, commit your work with git." ;;
    *) echo "unknown mode $MODE" >&2; exit 2 ;;
  esac
  PROMPT="$(python3 - "$ROOT/dogfood/baseline-prompt.md" "$NAME" "$MODE_TEXT" "$(task_list "$i")" "$FINISH" <<'PY'
import sys
template, name, mode, tasks, finish = sys.argv[1:6]
text = open(template).read()
print(text.replace("{name}", name).replace("{mode}", mode).replace("{tasks}", tasks).replace("{finish}", finish))
PY
)"
  mkdir -p "$RUN/$NAME"
  (cd "$CWD" && claude -p "$PROMPT" --model "$MODEL" ${TRELLIS_DOGFOOD_EFFORT:+--effort "$TRELLIS_DOGFOOD_EFFORT"} --tools "$TOOLS" --allowedTools "$TOOLS" \
    --strict-mcp-config --max-budget-usd "$BUDGET" --output-format json --no-session-persistence \
    >"$RUN/$NAME/result.json" 2>"$RUN/$NAME/stderr.log") &
  PIDS+=($!)
done
for pid in "${PIDS[@]}"; do wait "$pid" || true; done
ELAPSED=$(( $(date +%s) - START ))

MERGE_NOTES=""
REPAIR_S=0
REPAIR_COST=0
if [[ "$MODE" == worktrees* ]]; then
  for i in $(seq 1 "$AGENTS"); do
    WT="$WORK/agent-$i"
    if [ -n "$(git -C "$WT" status --porcelain)" ]; then
      git -C "$WT" add -A && git_c -C "$WT" commit -qm "uncommitted work left by agent-$i"
      MERGE_NOTES="$MERGE_NOTES agent-$i:auto-committed"
    fi
    if git_c -C "$WORK/main" merge -q --no-edit "agent-$i" >/dev/null 2>&1; then
      MERGE_NOTES="$MERGE_NOTES agent-$i:merged"
    else
      FILES="$(git -C "$WORK/main" diff --name-only --diff-filter=U | tr '\n' ',')"
      if [ -n "${TRELLIS_DOGFOOD_INTEGRATE:-}" ]; then
        # The merge stops with conflicts; one more agent resolves them, as a person would.
        mkdir -p "$RUN/resolver-$i"
        R_START=$(date +%s)
        (cd "$WORK/main" && claude -p "A git merge of branch agent-$i into this repository stopped with conflicts in: $FILES. Resolve every conflict so that both sides' changes are kept, run python3 -m unittest discover -s tests, and commit the merge with git. Stay inside this directory. Keep messages short." \
          --model "$MODEL" ${TRELLIS_DOGFOOD_EFFORT:+--effort "$TRELLIS_DOGFOOD_EFFORT"} --tools "$TOOLS" --allowedTools "$TOOLS" \
          --strict-mcp-config --max-budget-usd "$BUDGET" --output-format json --no-session-persistence \
          >"$RUN/resolver-$i/result.json" 2>"$RUN/resolver-$i/stderr.log") || true
        REPAIR_S=$(( REPAIR_S + $(date +%s) - R_START ))
        REPAIR_COST="$(python3 -c "import json,sys; print(float(sys.argv[1]) + (json.load(open(sys.argv[2])).get('total_cost_usd') or 0))" "$REPAIR_COST" "$RUN/resolver-$i/result.json" 2>/dev/null || echo "$REPAIR_COST")"
      fi
      if git -C "$WORK/main" rev-parse -q --verify MERGE_HEAD >/dev/null; then
        git -C "$WORK/main" merge --abort
        MERGE_NOTES="$MERGE_NOTES agent-$i:CONFLICT[$FILES]"
      else
        MERGE_NOTES="$MERGE_NOTES agent-$i:resolved[$FILES]"
      fi
    fi
  done
fi

TESTS="$(cd "$WORK/main" && python3 -m unittest discover -s tests 2>&1 | tail -1 || true)"
python3 "$ROOT/dogfood/baseline_report.py" "$RUN" "$WORK/main" "$MODE" "$ELAPSED" "$TESTS" "$MERGE_NOTES" | tee "$RUN/report.md"
[ "$REPAIR_S" -gt 0 ] && printf -- "- merge repair: %ss; cost: \$%.4f\n" "$REPAIR_S" "$REPAIR_COST" | tee -a "$RUN/report.md"
[ -n "${TRELLIS_DOGFOOD_INTEGRATE:-}" ] && "$ROOT/dogfood/integrate.sh" "$WORK/main" "$RUN" "$BUDGET" "$MODEL"
cp -R "$WORK/main" "$RUN/final"

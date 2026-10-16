#!/usr/bin/env bash
# The integration step every condition gets: if the combined result fails its tests, one more
# agent (Claude Code's own tools, can run the tests) fixes it. Its time and cost count toward the
# condition: what is compared is the time and money until the whole suite passes.
#
#   dogfood/integrate.sh <final_dir> <run_dir> [budget=1] [model=claude-sonnet-5]
#
# Appends `- integration: ...` to <run_dir>/report.md.
set -uo pipefail

FINAL="$1"
RUN="$2"
BUDGET="${3:-1}"
MODEL="${4:-claude-sonnet-5}"
TOOLS="Read,Write,Edit,Bash,Glob,Grep"

if (cd "$FINAL" && python3 -m unittest discover -s tests >/dev/null 2>&1); then
  echo "- integration: not needed" | tee -a "$RUN/report.md"
  exit 0
fi
mkdir -p "$RUN/integrator"
PROMPT="Several agents worked on this Python project separately and their work was combined; now the test suite fails. Fix the code so that \`python3 -m unittest discover -s tests\` passes. Do not change the tests. Stay inside this directory. Keep messages short."
START=$(date +%s)
(cd "$FINAL" && claude -p "$PROMPT" --model "$MODEL" ${TRELLIS_DOGFOOD_EFFORT:+--effort "$TRELLIS_DOGFOOD_EFFORT"} --tools "$TOOLS" --allowedTools "$TOOLS" \
  --strict-mcp-config --max-budget-usd "$BUDGET" --output-format json --no-session-persistence \
  >"$RUN/integrator/result.json" 2>"$RUN/integrator/stderr.log") || true
ELAPSED=$(( $(date +%s) - START ))
TESTS="$(cd "$FINAL" && python3 -m unittest discover -s tests 2>&1 | tail -1)"
COST="$(python3 -c "import json,sys; print(round(json.load(open(sys.argv[1])).get('total_cost_usd') or 0, 4))" "$RUN/integrator/result.json" 2>/dev/null || echo 0)"
echo "- integration: ${ELAPSED}s; cost: \$$COST; tests after: $TESTS" | tee -a "$RUN/report.md"

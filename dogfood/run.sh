#!/usr/bin/env bash
# Dogfooding: headless Claude Code agents work on a scenario together through Trellis (MCP
# only: no shell, no local files). Each agent has a hard spending cap.
#
#   dogfood/run.sh <scenario> [agents=3] [budget_usd_per_agent=1] [model=claude-sonnet-5] [variant]
#
# A scenario directory (dogfood/scenarios/<name>) holds seed/ (the initial files), policy.yaml,
# prompt.md ({name} and {agents} are substituted), and tasks.tsv (subject, detail, scope).
# A variant (for example `locked`) uses prompt-<variant>.md instead of prompt.md. Variants named
# `native...` give agents Claude Code's own tools (Read, Write, Edit, Bash, Glob, Grep) in an empty
# directory that `tl start` turns into a Trellis checkout; they land with `tl land`.
# TRELLIS_DOGFOOD_MAX_CLAIMS sets how many tasks one agent may hold at once (default 1).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCENARIO="${1:?usage: dogfood/run.sh <scenario> [agents] [budget] [model]}"
AGENTS="${2:-3}"
BUDGET="${3:-1}"
MODEL="${4:-claude-sonnet-5}"
VARIANT="${5:-}"
DIR_S="$ROOT/dogfood/scenarios/$SCENARIO"
PORT="${TRELLIS_DOGFOOD_PORT:-7480}"
RUN="${TRELLIS_DOGFOOD_DIR:-$ROOT/dogfood/runs/$SCENARIO${VARIANT:+-$VARIANT}-$(date +%Y%m%d-%H%M%S)}"
PROMPT_FILE="$DIR_S/prompt${VARIANT:+-$VARIANT}.md"
[ -f "$PROMPT_FILE" ] || { echo "no $PROMPT_FILE" >&2; exit 1; }
REPO="$SCENARIO"

mkdir -p "$RUN"
cargo build --release -q -p trellis-cli --manifest-path "$ROOT/Cargo.toml"
TL="$ROOT/target/release/tl"
export TRELLIS_SERVER="http://127.0.0.1:$PORT" TRELLIS_REPO="$REPO"

"$TL" serve --addr "127.0.0.1:$PORT" --data "$RUN/data" --policy "$DIR_S/policy.yaml" \
  --max-claims "${TRELLIS_DOGFOOD_MAX_CLAIMS:-1}" >"$RUN/server.log" 2>&1 &
SERVER=$!
trap 'kill "$SERVER" 2>/dev/null || true' EXIT
for _ in $(seq 50); do curl -s --noproxy '*' -o /dev/null "$TRELLIS_SERVER/repos/$REPO" && break; sleep 0.1; done
"$TL" init >/dev/null

# Seed through the Git bridge, then let the validator record its starting point.
cp -R "$DIR_S/seed" "$RUN/git"
git -C "$RUN/git" init -q
git -C "$RUN/git" add -A
git -C "$RUN/git" -c user.name=seed -c user.email=seed@example.com commit -qm "$SCENARIO skeleton"
"$TL" --as seed git import "$RUN/git" >"$RUN/import.json"
sleep 2
while IFS=$'\t' read -r subject detail scope; do
  [ -n "$subject" ] && "$TL" --as planner task create "$subject" --detail "$detail" --scope "$scope" >/dev/null
done <"$DIR_S/tasks.tsv"

START=$(date +%s)
PIDS=()
for i in $(seq 1 "$AGENTS"); do
  NAME="agent-$i"
  DIR="$RUN/$NAME"
  mkdir -p "$DIR"
  cat >"$DIR/mcp.json" <<JSON
{"mcpServers": {"trellis": {"command": "$TL", "args": ["mcp"],
  "env": {"TRELLIS_SERVER": "$TRELLIS_SERVER", "TRELLIS_REPO": "$REPO", "TRELLIS_PRINCIPAL": "$NAME"}}}}
JSON
  PROMPT="$(sed "s/{name}/$NAME/g; s/{agents}/$AGENTS/g" "$PROMPT_FILE")"
  if [[ "$VARIANT" == native* ]]; then
    TOOLS="Read,Write,Edit,Bash,Glob,Grep"
    mkdir -p "$DIR/work"
    (cd "$DIR/work" && PATH="$(dirname "$TL"):$PATH" TRELLIS_PRINCIPAL="$NAME" \
      claude -p "$PROMPT" --model "$MODEL" ${TRELLIS_DOGFOOD_EFFORT:+--effort "$TRELLIS_DOGFOOD_EFFORT"} --tools "$TOOLS" --allowedTools "$TOOLS" \
      --strict-mcp-config --max-budget-usd "$BUDGET" --output-format json --no-session-persistence \
      >"$DIR/result.json" 2>"$DIR/stderr.log") &
  else
    (cd "$DIR" && claude -p "$PROMPT" --model "$MODEL" ${TRELLIS_DOGFOOD_EFFORT:+--effort "$TRELLIS_DOGFOOD_EFFORT"} --tools "" \
      --mcp-config "$DIR/mcp.json" --strict-mcp-config --allowedTools "mcp__trellis" \
      --max-budget-usd "$BUDGET" --output-format json --no-session-persistence \
      >"$DIR/result.json" 2>"$DIR/stderr.log") &
  fi
  PIDS+=($!)
done
for pid in "${PIDS[@]}"; do wait "$pid" || true; done
ELAPSED=$(( $(date +%s) - START ))

sleep 2  # let the validator finish the last landing
"$TL" git export "$RUN/git" >"$RUN/export.json"
TESTS="$(cd "$RUN/git" && python3 -m unittest discover -s tests 2>&1 | tail -1 || true)"
python3 "$ROOT/dogfood/report.py" "$RUN" "$TRELLIS_SERVER" "$REPO" "$ELAPSED" "$TESTS" | tee "$RUN/report.md"
[ -n "${TRELLIS_DOGFOOD_INTEGRATE:-}" ] && "$ROOT/dogfood/integrate.sh" "$RUN/git" "$RUN" "$BUDGET" "$MODEL"
true

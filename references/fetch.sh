#!/usr/bin/env bash
# Re-runnable downloader for references. PDFs are validated (%%EOF) before being kept.
cd "$(dirname "$0")"
UA="Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 Chrome/130 Safari/537.36"
ok_pdf(){ head -c 5 "$1" | grep -q '%PDF-' && tail -c 2048 "$1" | grep -q '%%EOF'; }
dl(){ url=$1; out=$2
  if [ -s "$out" ]; then case "$out" in *.pdf) ok_pdf "$out" && { echo "ok(cached) $out"; return; };; *) echo "ok(cached) $out"; return;; esac; fi
  for i in 1 2 3; do
    curl -sL -A "$UA" --retry 2 --connect-timeout 20 --max-time 600 -o "$out.part" "$url" || { echo "retry $i $out"; continue; }
    case "$out" in *.pdf) ok_pdf "$out.part" || { echo "bad-pdf retry $i $out"; continue; };; esac
    mv "$out.part" "$out"; echo "ok $(wc -c <"$out") $out"; return
  done; rm -f "$out.part"; echo "FAIL $url"; }
mkdir -p papers articles mega sapling oak crdt coordination
dl https://arxiv.org/pdf/2608.23740 papers/2608.23740-AgentRoom-CRDT-multi-agent.pdf
dl https://arxiv.org/pdf/2606.15376 papers/2606.15376-CoAgent-concurrency-control.pdf
dl https://arxiv.org/pdf/2606.10662 papers/2606.10662-DeLM-shared-context.pdf
dl https://arxiv.org/pdf/2607.00041 papers/2607.00041-ATM-CID-brokered-admission.pdf
dl https://arxiv.org/pdf/2605.18747 papers/2605.18747-code-as-agent-harness.pdf
dl https://arxiv.org/pdf/2404.11943 papers/2404.11943-AgentCoord.pdf
dl https://arxiv.org/pdf/2609.35759 papers/2609.35759-NstAgent-narrative-state-tracking.pdf
dl https://arxiv.org/pdf/2609.32297 papers/2609.32297-Agentsensus-shared-memory-story-worlds.pdf
dl https://arxiv.org/pdf/2607.00918 papers/2607.00918-Magnet-Atlas-personas-to-plot.pdf
dl https://arxiv.org/pdf/2608.05169 papers/2608.05169-ConWriter.pdf
dl https://arxiv.org/pdf/2410.02603 papers/2410.02603-Agents-Room-narrative.pdf
dl https://arxiv.org/pdf/2506.16445 papers/2506.16445-StoryWriter.pdf
dl https://sunshowers.io/posts/anthropic-mononoke/ articles/sunshowers-scion-inspired-by-mononoke.html
dl https://hackernoon.com/ai-agents-are-killing-the-git-client-not-git-what-anthropics-scion-and-cursors-origin-show articles/hackernoon-scion-vs-cursor-origin.html
dl https://raw.githubusercontent.com/facebook/sapling/main/eden/mononoke/README.md sapling/mononoke-README.md
dl https://raw.githubusercontent.com/gitmono-dev/scorpiofs/main/README.md mega/scorpiofs-README.md
dl https://raw.githubusercontent.com/web3infra-foundation/libra/main/README.md mega/libra-README.md
dl https://raw.githubusercontent.com/Dicklesworthstone/mcp_agent_mail_rust/main/README.md coordination/mcp-agent-mail-rust-README.md
dl https://raw.githubusercontent.com/loro-dev/loro/main/README.md crdt/loro-README.md
dl https://raw.githubusercontent.com/automerge/automerge/main/README.md crdt/automerge-README.md
dl https://raw.githubusercontent.com/y-crdt/y-crdt/main/README.md crdt/yrs-README.md
dl https://raw.githubusercontent.com/jj-vcs/jj/main/docs/technical/concurrency.md crdt/jj-concurrency.md
dl https://raw.githubusercontent.com/jj-vcs/jj/main/docs/operation-log.md crdt/jj-operation-log.md
echo DONE

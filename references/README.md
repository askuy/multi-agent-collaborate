# References

Local copies of the sources cited in the research report [`docs/research/2026-10-15-multi-agent-collaboration-substrate.md`](../docs/research/2026-10-15-multi-agent-collaboration-substrate.md).
Downloaded 2026-10-15. Re-runnable script: [`fetch.sh`](fetch.sh). It skips files already downloaded, and a PDF is only kept if it passes the `%%EOF` check.

> Copyright stays with the original authors. This folder is for internal research reading only. Papers come from arXiv; articles are saved as the original HTML plus a pandoc-converted `.md`.

## articles/ — Scion and its coverage

| File | Source | Notes |
|---|---|---|
| `anthropic-redesigning-source-control.{html,md}` | [Anthropic Engineering, 2026-10-13](https://www.anthropic.com/engineering/redesigning-source-control) | **Original article; read first.** Changesets, CAS on a single Spanner row, derived data, three-tier cache, abandoning the merge queue |
| `simonwillison-scion.{html,md}` | [Simon Willison](https://simonwillison.net/2026/Oct/13/redesigning-source-control-at-anthropic/) | Subversion comparison; HN quote from Nick G. on dropping offline support |
| `theregister-scion.{html,md}` | [The Register](https://www.theregister.com/devops/2026/10/13/anthropic-reinvents-source-control-as-claude-floods-the-world-with-code/5302943) | Comparison with SVN / Perforce / Piper / VFS for Git / Sapling |
| `sunshowers-scion-inspired-by-mononoke.{html,md}` | [sunshowers.io](https://sunshowers.io/posts/anthropic-mononoke/) | Former Mononoke engineer: Scion is heavily inspired by Bonsai changesets + derived data |
| `hackernoon-scion-vs-cursor-origin.{html,md}` | [HackerNoon](https://hackernoon.com/ai-agents-are-killing-the-git-client-not-git-what-anthropics-scion-and-cursors-origin-show) | Scion (drops Git) vs Cursor Origin (keeps Git, replaces GitHub) |

## mega/ — Mega / Libra / ScorpioFS

From [gitmono-dev/mega](https://github.com/gitmono-dev/mega) (main branch, 2026-10-15); file names map `/` to `_`.

| File | Notes |
|---|---|
| `README.md` | Positioning: an open-source Piper, Git-compatible, Mega + Libra |
| `docs_architecture.md` | Crate breakdown, runtime assembly, layering rules |
| `ceres_README.md` | Domain-layer design: transport/application split, event bus, CI-enforced dependency rules (**worth copying**) |
| `docs_agent.md` | Mega's own AI/MCP adoption plan (Chinese) |
| `docs_artifacts-protocol.md`, `docs_decentralize.md` | Artifacts protocol, P2P collaboration events |
| `orion_README.md`, `vault_README.md`, `mono_README.md`, ... | Other modules |
| `libra-README.md` | [Libra](https://github.com/web3infra-foundation/libra): agent session trace capture (`refs/libra/traces`) |
| `scorpiofs-README.md` | [ScorpioFS](https://github.com/gitmono-dev/scorpiofs): FUSE + Antares three-layer overlay |

## sapling/

| File | Notes |
|---|---|
| `mononoke-README.md` | [Mononoke](https://github.com/facebook/sapling/tree/main/eden/mononoke) server overview. For depth, read `derived_data/` and `mononoke_types/` in the source |

## oak/

| File | Notes |
|---|---|
| `oak-blog.{html,md}` | [Oak dev blog](https://oak.space/blog): receipts, CAS merges, review without checkout, removal of the orchestration layer |

## coordination/ — multi-agent coordination tools

| File | Notes |
|---|---|
| `mcp-agent-mail-rust-README.md` | [MCP Agent Mail (Rust)](https://github.com/Dicklesworthstone/mcp_agent_mail_rust): identities, inboxes, advisory file leases + pre-commit guard |

## crdt/ — CRDTs and version control semantics

| File | Notes |
|---|---|
| `loro-README.md` | [Loro](https://github.com/loro-dev/loro): Rust CRDT, checkout/fork/shallow snapshots |
| `automerge-README.md`, `yrs-README.md` | Automerge, Yrs (Rust port of Y.js) |
| `jj-concurrency.md`, `jj-operation-log.md` | Jujutsu: concurrent operations, operation log, first-class conflicts |

## papers/ (12 papers, all verified to open with macOS PDFKit)

| File | Topic | Core takeaway |
|---|---|---|
| `2608.23740-AgentRoom-CRDT-multi-agent.pdf` | CRDT shared workspace + MCP claims (ICML 2026) | Coordination drives quality; byte merging ≠ semantic merging; one broadcast channel fails past ×3 |
| `2606.15376-CoAgent-concurrency-control.pdf` | Database concurrency control for multi-agent systems | Read/write sets, OCC, locks as a fallback |
| `2607.00041-ATM-CID-brokered-admission.pdf` | Pre-write admission | Intent → semantic atoms; a neutral steward lands all writes |
| `2606.10662-DeLM-shared-context.pdf` | Decentralized shared context + task queue | +17.5 points accuracy, 2.49× faster |
| `2605.18747-code-as-agent-harness.pdf` | Survey: code as agent harness | External persistent structures that work like version control |
| `2609.35759-NstAgent-narrative-state-tracking.pdf` | Narrative state tracking | Structured state beats rolling summaries from 20K–100K words |
| `2609.32297-Agentsensus-shared-memory-story-worlds.pdf` | Consensus-compressed shared memory | Merging duplicate events: 22–44% fewer entries |
| `2607.00918-Magnet-Atlas-personas-to-plot.pdf` | Character agents + world-state consistency checks | Atlas compares world-state graphs between scenes |
| `2608.05169-ConWriter.pdf` | Neuro-symbolic consistency control | State transition constraints |
| `2506.16445-StoryWriter.pdf` | Multi-agent long-story generation | Outline / plan / write layers |
| `2410.02603-Agents-Room-narrative.pdf` | Agents' Room | Shared scratchpad |
| `2404.11943-AgentCoord.pdf` | Visualizing coordination strategies | Example of coordinating multiple agents writing a novel |

### Not downloaded (needs to be fetched manually)

- **Google Piper**: Potvin & Levenberg, *Why Google Stores Billions of Lines of Code in a Single Repository*, CACM 2016. The ACM PDF ([doi 10.1145/2854146](https://dl.acm.org/doi/10.1145/2854146)) has anti-bot protection that blocks scripted downloads. Open it in a browser and save it as `papers/google-piper-cacm-2016.pdf`, or read the [CACM web version](https://cacm.acm.org/research/why-google-stores-billions-of-lines-of-code-in-a-single-repository/).
- **Oak source**: hosted on [oak.space/oak/oak](https://oak.space/oak/oak) (not GitHub); install `oakvcs-cli` from crates.io to see the implementation.

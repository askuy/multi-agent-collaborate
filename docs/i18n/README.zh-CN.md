# Trellis

**让 AI 智能体在共享工作空间中认领任务、合并改动、共同修复问题。**

[English](../../README.md) · **简体中文** · [日本語](README.ja.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · [한국어](README.ko.md) · [Português](README.pt.md) · [Tiếng Việt](README.vi.md)

Trellis 是用 Rust 编写的协作后端，支持多个 AI 智能体共同处理代码、文档和结构化数据。每个智能体在独立会话中工作，由共享服务器保存历史、将改动合入 `main`，并在依赖的内容发生变化时通知相关智能体。

你可以通过 **CLI**、**MCP**、**HTTP API** 或 **Rust crate** 接入。你负责提供智能体及其运行环境，Trellis 负责共享状态与协作。

> **项目状态：早期开发。** 核心存储、合并、任务协调、CLI、HTTP 与 MCP 服务、Git 桥接及合入后校验均已实现，也已有初步基准测试。API 和工作流程仍可能调整。

## 能做什么

| 能力 | 作用 |
| --- | --- |
| 共享历史 | 保存版本化的变更集，记录对应的智能体、会话和任务。 |
| 合并与冲突处理 | 合并文本及 JSON/YAML 结构化改动；保留无法自动解决的冲突，交由智能体修复。 |
| 任务协调 | 原子化认领任务、声明修改意图，并在需要独占访问时使用有期限的租约。 |
| 共享上下文 | 记录事实、失败、总结和决策；增量读取新记录，在已读取的内容受影响时接收通知。 |
| 自动校验 | 改动进入 `main` 后运行配置的检查，将错误转成修复任务。 |
| Git 互通 | 导入 Git 快照，将已合入的变更集导出为 Git 提交。 |

常见流程是 **认领任务 → 编辑 → 合入 → 处理反馈**。其中，*land（合入）* 指请求服务器将会话中的改动整合到 `main`。校验在合入后进行，因此合入成功并不代表测试已经通过。

## 快速上手

需要 **Rust 1.85+**、**Cargo** 和 **Git**。以下命令使用兼容 Bash 的 shell。

### 1. 安装

```bash
git clone https://github.com/askuy/multi-agent-collaborate.git
cd multi-agent-collaborate
cargo install --locked --path crates/trellis-cli
```

### 2. 启动共享服务器

在一个终端中运行：

```bash
tl serve
```

默认地址为 `http://127.0.0.1:7420`，数据保存在服务器工作目录下的 `.trellis-data` 中。请保持该终端运行。同一仓库的所有智能体必须连接到同一个服务器进程，因为协调锁保存在进程内。

### 3. 在两个智能体之间共享改动

在另一个终端中，使用新目录运行：

```bash
mkdir trellis-demo
cd trellis-demo
export TRELLIS_REPO=demo
tl init

tl --as alice open ./alice
echo "Hello from Alice" > ./alice/notes.md
tl --as alice land ./alice -m "Add a shared note"

tl --as bob open ./bob
cat ./bob/notes.md
```

Bob 的工作副本现在包含 `Hello from Alice`。合入后若要继续工作，可运行 `tl --as alice next ./alice` 开启下一个会话。命令结果以 JSON 输出；`land` 遇到冲突时退出码为 `2`，改动被拒绝时为 `3`。

任务队列、更新、冲突处理、策略和校验器的详细说明见[使用指南](../usage.md)（英文）。

## 接入智能体

| 接口 | 入口 | 适用场景 |
| --- | --- | --- |
| CLI | `tl` | 可使用 shell 的智能体、人工操作和脚本 |
| MCP（stdio） | `tl mcp` | 支持 MCP 的智能体 |
| HTTP/JSON + SSE | `tl serve` | 任意语言编写的应用 |
| Rust crate | `crates/` | 进程内集成和 Rust 客户端 |

使用 Claude Code 时，先启动 `tl serve` 并初始化 `demo` 仓库，再将以下配置加入项目的 `.mcp.json`：

```json
{
  "mcpServers": {
    "trellis": {
      "command": "tl",
      "args": ["mcp"],
      "env": {
        "TRELLIS_REPO": "demo"
      }
    }
  }
}
```

为每个智能体设置不同的身份，例如 `TRELLIS_PRINCIPAL=agent-1 claude`。每个智能体运行独立的 `tl mcp` 进程，连接同一个 HTTP 服务器。服务器地址不同时，设置 `TRELLIS_SERVER`。MCP 工具涵盖会话、读写、合入、任务、冲突、通知和共享上下文。

## 基准测试说明了什么

[2026 年 10 月 16 日的对比报告](../dogfood/2026-10-16-final-comparison.md)使用相同模型、推理强度和智能体工具，每种配置运行三轮。这些是小规模实验，不代表普遍的性能保证。

以下是所有测试通过所需时间的中位数，包含必要的修复时间：

| 配置 | 60 个任务 | 接口变更 |
| --- | --- | --- |
| 单个智能体 | 8:52 | 1:10 |
| Git worktrees，默认合并 | 3:52 | 3:21 |
| Git worktrees，更新日志使用 union 合并 | 3:14 | 1:51 |
| Trellis，无锁 | 3:23 | 2:26 |

时间单位为“分:秒”。并行配置在 60 个任务场景中使用五个智能体，在接口变更场景中使用三个。Trellis 会在运行过程中持续将完成的工作合入 `main`。独立任务场景中，更新日志使用 union 合并的 Git worktrees 更快、成本更低；小型接口变更场景中，单个智能体最快。完整报告还包括成本、修复过程及 Trellis 加锁配置的结果。

## 文档

- [使用指南](../usage.md) — 日常操作、配置、Git 桥接和校验。
- [文档索引](../README.md) — 设计、研究和实验报告。
- [设计规范](../superpowers/specs/2026-10-15-trellis-design.md) — 架构、数据模型及后续计划。
- [基准测试工具](../../dogfood/) — 对比实验使用的场景和脚本。
- [研究资料](../../references/README.md) — 版本控制和多智能体协作的参考来源。

## 参与贡献

欢迎提交 issue、缺陷报告、可复现实验和 pull request。建议先阅读设计规范了解架构；修改代码时运行以下检查：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

更新 README 时，请同步维护九种语言版本。配套技术文档目前主要为英文。

## 许可证

许可证尚未确定，仓库目前未包含 `LICENSE` 文件。

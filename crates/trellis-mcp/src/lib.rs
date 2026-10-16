//! An MCP server over stdio (JSON-RPC 2.0, one message per line) that lets an agent such as
//! Claude Code or Codex use Trellis (spec §5.2). It is a thin proxy: every tool call goes to
//! the one shared Trellis server over HTTP, so any number of agents can use it safely.

mod protocol;
mod tools;

pub use protocol::{SUPPORTED_VERSIONS, Server, serve_stdio};
pub use tools::INSTRUCTIONS;

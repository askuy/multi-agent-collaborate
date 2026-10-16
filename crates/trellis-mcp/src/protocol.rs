use std::io::{self, BufRead, Write};

use serde_json::{Value, json};
use trellis_client::Client;

use crate::tools::{self, Agent};

/// MCP protocol versions this server speaks, newest first.
pub const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// One agent's MCP server.
pub struct Server {
    agent: Agent,
}

fn response(id: Value, result: Result<Value, (i64, String)>) -> Value {
    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    }
}

impl Server {
    pub fn new(client: Client, principal: impl Into<String>) -> Self {
        Self {
            agent: Agent::new(client, principal.into()),
        }
    }

    /// Handles one JSON-RPC message: the response for a request, `None` for a notification.
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned()?;
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => Ok(initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                Ok(self.agent.call(name, &arguments))
            }
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(response(id, result))
    }
}

fn initialize(params: &Value) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let version = if SUPPORTED_VERSIONS.contains(&requested) {
        requested
    } else {
        SUPPORTED_VERSIONS[0]
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "trellis", "version": env!("CARGO_PKG_VERSION") },
        "instructions": tools::INSTRUCTIONS,
    })
}

/// Serves MCP over stdin/stdout until stdin closes.
pub fn serve_stdio(mut server: Server) -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => server.handle(&message),
            Err(e) => Some(response(
                Value::Null,
                Err((-32700, format!("parse error: {e}"))),
            )),
        };
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

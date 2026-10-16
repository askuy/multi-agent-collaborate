//! `tl mcp` speaks newline-delimited JSON-RPC over stdio.

use std::io::{BufRead, BufReader, Lines, Write};
use std::process::{ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

fn send(stdin: &mut ChildStdin, message: &Value) {
    writeln!(stdin, "{message}").unwrap();
}

fn request(
    stdin: &mut ChildStdin,
    lines: &mut Lines<BufReader<ChildStdout>>,
    message: &Value,
) -> Value {
    send(stdin, message);
    serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap()
}

#[test]
fn tl_mcp_answers_over_stdio() {
    let tmp = tempfile::tempdir().unwrap();
    let bg = trellis_server::start_background(trellis_server::Config::new(
        "127.0.0.1:0".parse().unwrap(),
        tmp.path().join("data"),
    ))
    .unwrap();
    trellis_client::Client::new(&bg.url(), "demo")
        .unwrap()
        .init()
        .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_tl"))
        .args([
            "--server",
            &bg.url(),
            "--repo",
            "demo",
            "--as",
            "agent-1",
            "mcp",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();

    let init = request(
        &mut stdin,
        &mut lines,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }),
    );
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "trellis");
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    );
    let opened = request(
        &mut stdin,
        &mut lines,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "session_open", "arguments": {} } }),
    );
    assert_eq!(opened["id"], 2);
    assert_eq!(opened["result"]["isError"], false);
    let written = request(
        &mut stdin,
        &mut lines,
        &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "write", "arguments": { "path": "a.md", "content": "hi" } } }),
    );
    assert_eq!(written["id"], 3);
    assert_eq!(written["result"]["isError"], false);

    drop(stdin);
    assert!(child.wait().unwrap().success());
}

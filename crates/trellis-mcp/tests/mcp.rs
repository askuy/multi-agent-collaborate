//! The MCP protocol and tools against a real Trellis server.

use serde_json::{Value, json};
use trellis_client::Client;
use trellis_mcp::Server;
use trellis_server::{Background, Config, start_background};

fn server() -> (Background, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let bg = start_background(Config::new(
        "127.0.0.1:0".parse().unwrap(),
        dir.path().join("data"),
    ))
    .unwrap();
    Client::new(&bg.url(), "demo").unwrap().init().unwrap();
    (bg, dir)
}

fn agent(bg: &Background, name: &str) -> Server {
    Server::new(Client::new(&bg.url(), "demo").unwrap(), name)
}

fn call(server: &mut Server, name: &str, args: Value) -> (String, bool) {
    let reply = server
        .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }))
        .unwrap();
    let result = &reply["result"];
    (
        result["content"][0]["text"].as_str().unwrap().to_owned(),
        result["isError"].as_bool().unwrap(),
    )
}

fn ok(server: &mut Server, name: &str, args: Value) -> String {
    let (text, is_error) = call(server, name, args);
    assert!(!is_error, "{name} failed: {text}");
    text
}

#[test]
fn handshake_and_tool_listing() {
    let (bg, _dir) = server();
    let mut s = agent(&bg, "a");
    let init = s
        .handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } } }))
        .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(init["result"]["serverInfo"]["name"], "trellis");
    assert!(
        init["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("session_open")
    );
    let newer = s.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2099-01-01" } })).unwrap();
    assert_eq!(newer["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(
        s.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })),
        None
    );
    assert_eq!(
        s.handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }))
            .unwrap()["result"],
        json!({})
    );

    let tools = s
        .handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }))
        .unwrap();
    let tools = tools["result"]["tools"].as_array().unwrap().clone();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "session_open",
        "read",
        "write",
        "land",
        "conflict_resolve",
        "intent",
        "task_claim",
        "log_append",
    ] {
        assert!(names.contains(&expected), "missing {expected}");
    }
    assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object"));

    let unknown = s
        .handle(&json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/list" }))
        .unwrap();
    assert_eq!(unknown["error"]["code"], -32601);
    let (text, is_error) = call(&mut s, "nope", json!({}));
    assert!(is_error && text.contains("unknown tool"));
}

#[test]
fn an_agent_reads_writes_and_lands_through_tools() {
    let (bg, _dir) = server();
    let mut a = agent(&bg, "alice");
    let (text, is_error) = call(&mut a, "read", json!({ "path": "x.md" }));
    assert!(is_error && text.contains("session_open"));
    ok(&mut a, "session_open", json!({}));
    ok(
        &mut a,
        "write",
        json!({ "path": "book/ch1.md", "content": "Once upon a time.\n" }),
    );
    assert_eq!(
        ok(&mut a, "read", json!({ "path": "book/ch1.md" })),
        "Once upon a time.\n"
    );
    assert_eq!(ok(&mut a, "list", json!({})), "book/ch1.md (changed)");
    let landed = ok(&mut a, "land", json!({ "message": "chapter 1" }));
    assert!(landed.contains("\"outcome\": \"landed\""), "{landed}");
    assert!(landed.contains("session_open for more work"));
    let main = Client::new(&bg.url(), "demo")
        .unwrap()
        .read_main("book/ch1.md")
        .unwrap();
    assert_eq!(main.as_deref(), Some(&b"Once upon a time.\n"[..]));
}

#[test]
fn notifications_ride_along_with_tool_results_once() {
    let (bg, _dir) = server();
    let mut a = agent(&bg, "alice");
    let mut b = agent(&bg, "bob");
    ok(&mut a, "session_open", json!({}));
    ok(
        &mut a,
        "write",
        json!({ "path": "doc.md", "content": "v1\n" }),
    );
    ok(&mut a, "land", json!({ "message": "v1" }));

    ok(&mut b, "session_open", json!({}));
    ok(&mut b, "read", json!({ "path": "doc.md" }));
    ok(&mut a, "session_open", json!({}));
    ok(
        &mut a,
        "write",
        json!({ "path": "doc.md", "content": "v2\n" }),
    );
    ok(&mut a, "land", json!({ "message": "rewrite doc" }));

    let listed = ok(&mut b, "list", json!({}));
    assert!(listed.contains("[trellis] 1 change(s)"), "{listed}");
    assert!(listed.contains("alice landed \"rewrite doc\""), "{listed}");
    assert!(!ok(&mut b, "list", json!({})).contains("[trellis]"));
    let notes: Value =
        serde_json::from_str(&ok(&mut b, "notifications", json!({ "ack": true }))).unwrap();
    assert_eq!(notes.as_array().unwrap().len(), 1);
    let rebased = ok(&mut b, "rebase", json!({}));
    assert!(rebased.contains("rebased"), "{rebased}");
    assert_eq!(ok(&mut b, "read", json!({ "path": "doc.md" })), "v2\n");
}

#[test]
fn conflicts_tasks_and_the_log_through_tools() {
    let (bg, _dir) = server();
    let mut a = agent(&bg, "alice");
    let mut b = agent(&bg, "bob");
    ok(&mut a, "session_open", json!({}));
    ok(
        &mut a,
        "write",
        json!({ "path": "doc.md", "content": "one\ntwo\nthree\n" }),
    );
    ok(&mut a, "land", json!({ "message": "seed" }));
    ok(&mut a, "session_open", json!({}));
    ok(&mut b, "session_open", json!({}));
    ok(
        &mut a,
        "write",
        json!({ "path": "doc.md", "content": "one\nTWO-A\nthree\n" }),
    );
    ok(
        &mut b,
        "write",
        json!({ "path": "doc.md", "content": "one\nTWO-B\nthree\n" }),
    );
    ok(&mut a, "land", json!({ "message": "a" }));
    let landing = ok(&mut b, "land", json!({ "message": "b" }));
    let conflicted: Value =
        serde_json::from_str(landing.split("\n\n[trellis]").next().unwrap()).unwrap();
    let id = conflicted["receipt"]["outcome"]["conflict"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(ok(&mut b, "conflict_show", json!({ "id": id })).contains("<<<<<<<"));
    ok(
        &mut b,
        "conflict_resolve",
        json!({ "id": id, "resolutions": { "doc.md": "one\nTWO-A+B\nthree\n" } }),
    );
    assert!(
        ok(&mut b, "land", json!({ "message": "b resolved" })).contains("\"outcome\": \"landed\"")
    );

    let task: Value = serde_json::from_str(&ok(
        &mut a,
        "task_create",
        json!({ "subject": "chapter 2", "scope": ["book/ch2/**"] }),
    ))
    .unwrap();
    let task_id = task["id"].as_str().unwrap();
    assert!(
        ok(&mut a, "task_claim", json!({ "id": task_id })).contains("\"outcome\": \"claimed\"")
    );
    assert!(ok(&mut b, "task_claim", json!({ "id": task_id })).contains("\"outcome\": \"taken\""));
    ok(
        &mut a,
        "task_complete",
        json!({ "id": task_id, "summary": "done" }),
    );

    ok(
        &mut a,
        "log_append",
        json!({ "kind": "fail", "body": "timeline broken", "topics": ["ch1"] }),
    );
    assert!(ok(&mut b, "log_read", json!({ "topics": ["ch1"] })).contains("timeline broken"));
    let (text, is_error) = call(
        &mut b,
        "log_append",
        json!({ "kind": "rumor", "body": "x" }),
    );
    assert!(is_error, "{text}");

    ok(&mut a, "session_open", json!({}));
    let lease = ok(
        &mut a,
        "intent",
        json!({ "description": "own ch3", "writes": ["book/ch3/**"], "exclusive": true }),
    );
    assert!(lease.contains("admitted"), "{lease}");
    assert!(ok(&mut b, "leases", json!({})).contains("own ch3"));
}

#[test]
fn a_task_takes_four_calls_next_read_write_land() {
    let (bg, _dir) = server();
    let planner = Client::new(&bg.url(), "demo").unwrap();
    let t1 = planner.create_task("planner", "slugify", "", &[]).unwrap();
    let t2 = planner.create_task("planner", "roman", "", &[]).unwrap();
    let mut a = agent(&bg, "alice");
    let mut b = agent(&bg, "bob");

    let next = ok(&mut a, "task_next", json!({}));
    assert!(
        next.contains(&t1.id) && next.contains("\"session\""),
        "{next}"
    );
    assert!(ok(&mut b, "task_next", json!({})).contains(&t2.id));

    ok(
        &mut a,
        "write",
        json!({ "files": { "src/slugify.py": "def slugify(): ...\n", "CHANGELOG.md": "- slugify\n" } }),
    );
    let both = ok(
        &mut a,
        "read",
        json!({ "paths": ["src/slugify.py", "CHANGELOG.md", "missing.md"] }),
    );
    assert!(
        both.contains("=== src/slugify.py ===\ndef slugify(): ...\n"),
        "{both}"
    );
    assert!(both.contains("=== CHANGELOG.md ===\n- slugify\n"), "{both}");
    assert!(
        both.contains("=== missing.md === (does not exist)"),
        "{both}"
    );

    let landed = ok(
        &mut a,
        "land",
        json!({ "message": "slugify", "summary": "implemented slugify" }),
    );
    assert!(landed.contains("\"outcome\": \"landed\""), "{landed}");
    assert!(landed.contains("\"completed\""), "{landed}");
    let tasks = planner.tasks().unwrap();
    let done = tasks.iter().find(|t| t.id == t1.id).unwrap();
    assert!(
        matches!(done.status, trellis_coord::TaskStatus::Done { .. }),
        "{done:?}"
    );

    assert!(ok(&mut a, "task_next", json!({})).contains("no open task"));
}

#[test]
fn an_exclusive_intent_can_wait_for_the_lock_and_then_rebase() {
    let (bg, _dir) = server();
    let mut a = agent(&bg, "alice");
    let mut b = agent(&bg, "bob");
    ok(&mut a, "session_open", json!({}));
    ok(&mut b, "session_open", json!({}));
    let lock = json!({ "description": "append", "writes": ["CHANGELOG.md"], "exclusive": true });
    assert!(ok(&mut a, "intent", lock.clone()).contains("admitted"));
    let (busy, _) = call(&mut b, "intent", lock.clone());
    assert!(busy.contains("conflict"), "{busy}");

    let waiter = std::thread::spawn(move || {
        let mut args = lock;
        args["wait_seconds"] = json!(10);
        args["rebase"] = json!(true);
        let text = ok(&mut b, "intent", args);
        let seen = ok(&mut b, "read", json!({ "path": "CHANGELOG.md" }));
        (text, seen)
    });
    std::thread::sleep(std::time::Duration::from_millis(500));
    ok(
        &mut a,
        "write",
        json!({ "path": "CHANGELOG.md", "content": "- a\n" }),
    );
    ok(&mut a, "land", json!({ "message": "a" }));
    let (text, seen) = waiter.join().unwrap();
    assert!(
        text.contains("admitted") && text.contains("waited_ms"),
        "{text}"
    );
    assert!(text.contains("rebased"), "{text}");
    assert_eq!(seen, "- a\n");
}

#[test]
fn append_adds_a_line_without_sending_the_file() {
    let (bg, _dir) = server();
    let mut a = agent(&bg, "alice");
    ok(&mut a, "session_open", json!({}));
    ok(
        &mut a,
        "write",
        json!({ "path": "CHANGELOG.md", "content": "## Unreleased\n" }),
    );
    ok(
        &mut a,
        "append",
        json!({ "path": "CHANGELOG.md", "text": "- one" }),
    );
    ok(
        &mut a,
        "append",
        json!({ "path": "CHANGELOG.md", "text": "- two\n" }),
    );
    ok(
        &mut a,
        "append",
        json!({ "path": "new.md", "text": "first" }),
    );
    assert_eq!(
        ok(&mut a, "read", json!({ "path": "CHANGELOG.md" })),
        "## Unreleased\n- one\n- two\n"
    );
    assert_eq!(ok(&mut a, "read", json!({ "path": "new.md" })), "first\n");
}

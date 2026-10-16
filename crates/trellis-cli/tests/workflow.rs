//! Two agents collaborating through `tl` against a real server.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn tl(server: &str, principal: &str, args: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_tl"))
        .args(["--server", server, "--repo", "demo", "--as", principal])
        .args(args)
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert!(
        matches!(code, 0 | 2 | 3),
        "tl {args:?} failed ({code}): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        code,
        serde_json::from_slice(&out.stdout).unwrap_or(Value::Null),
    )
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn two_agents_collaborate_through_checkouts() {
    let tmp = tempfile::tempdir().unwrap();
    let bg = trellis_server::start_background(trellis_server::Config::new(
        "127.0.0.1:0".parse().unwrap(),
        tmp.path().join("data"),
    ))
    .unwrap();
    let url = bg.url();
    let (a, b, c) = (
        tmp.path().join("alice"),
        tmp.path().join("bob"),
        tmp.path().join("carol"),
    );

    tl(&url, "planner", &["init"]);
    let (_, task) = tl(
        &url,
        "planner",
        &["task", "create", "chapter 1", "--scope", "book/**"],
    );
    let task_id = task["id"].as_str().unwrap().to_owned();
    let (_, claim) = tl(&url, "alice", &["task", "claim", &task_id]);
    assert_eq!(claim["outcome"], "claimed");

    tl(&url, "alice", &["open", path(&a), "--task", &task_id]);
    std::fs::create_dir_all(a.join("book")).unwrap();
    std::fs::write(a.join("book/ch1.md"), "one\ntwo\nthree\n").unwrap();
    let (code, receipt) = tl(&url, "alice", &["land", path(&a), "-m", "chapter 1"]);
    assert_eq!(
        (code, receipt["outcome"]["outcome"].as_str()),
        (0, Some("landed"))
    );

    tl(&url, "bob", &["open", path(&b)]);
    assert_eq!(
        std::fs::read_to_string(b.join("book/ch1.md")).unwrap(),
        "one\ntwo\nthree\n"
    );

    tl(&url, "alice", &["next", path(&a)]);
    std::fs::write(a.join("book/ch1.md"), "ONE\ntwo\nthree\n").unwrap();
    std::fs::write(b.join("book/ch1.md"), "one\ntwo\nTHREE\n").unwrap();
    tl(&url, "alice", &["land", path(&a), "-m", "alice edits"]);
    let (_, status) = tl(&url, "bob", &["status", path(&b)]);
    assert_eq!(status["notifications"].as_array().unwrap().len(), 1);
    assert_eq!(status["changes"]["written"][0], "book/ch1.md");
    let (code, receipt) = tl(&url, "bob", &["land", path(&b), "-m", "bob edits"]);
    assert_eq!(
        (code, receipt["outcome"]["route"].as_str()),
        (0, Some("composed"))
    );

    // A conflict, resolved through the checkout.
    tl(&url, "alice", &["next", path(&a)]);
    tl(&url, "bob", &["next", path(&b)]);
    std::fs::write(a.join("book/ch1.md"), "ONE\nTWO-A\nTHREE\n").unwrap();
    std::fs::write(b.join("book/ch1.md"), "ONE\nTWO-B\nTHREE\n").unwrap();
    tl(&url, "alice", &["land", path(&a), "-m", "alice line 2"]);
    let (code, receipt) = tl(&url, "bob", &["land", path(&b), "-m", "bob line 2"]);
    assert_eq!(code, 2);
    let conflict = receipt["outcome"]["conflict"].as_str().unwrap().to_owned();
    tl(&url, "bob", &["conflict", "markers", path(&b), &conflict]);
    std::fs::write(b.join("book/ch1.md"), "ONE\nTWO-A+B\nTHREE\n").unwrap();
    tl(&url, "bob", &["conflict", "resolve", path(&b), &conflict]);
    let (code, _) = tl(&url, "bob", &["land", path(&b), "-m", "bob resolved"]);
    assert_eq!(code, 0);

    tl(&url, "carol", &["open", path(&c)]);
    assert_eq!(
        std::fs::read_to_string(c.join("book/ch1.md")).unwrap(),
        "ONE\nTWO-A+B\nTHREE\n"
    );

    tl(
        &url,
        "alice",
        &[
            "log",
            "add",
            "chapter 1 drafted",
            "--kind",
            "summary",
            "--topic",
            "ch1",
        ],
    );
    let (_, entries) = tl(&url, "carol", &["log", "read", "--topic", "ch1"]);
    assert_eq!(entries.as_array().unwrap().len(), 1);
    let (_, done) = tl(
        &url,
        "alice",
        &["task", "complete", &task_id, "--summary", "drafted"],
    );
    assert_eq!(done["status"]["state"], "done");
}

#[test]
fn start_takes_the_next_task_and_land_with_a_summary_completes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let bg = trellis_server::start_background(trellis_server::Config::new(
        "127.0.0.1:0".parse().unwrap(),
        tmp.path().join("data"),
    ))
    .unwrap();
    let url = bg.url();
    let (seed, a, check) = (
        tmp.path().join("seed"),
        tmp.path().join("alice"),
        tmp.path().join("check"),
    );
    tl(&url, "planner", &["init"]);
    tl(&url, "seed", &["open", path(&seed)]);
    std::fs::write(seed.join(".gitignore"), "__pycache__/\n*.pyc\n").unwrap();
    std::fs::write(seed.join("a.txt"), "a\n").unwrap();
    tl(&url, "seed", &["land", path(&seed), "-m", "seed"]);
    let (_, t1) = tl(&url, "planner", &["task", "create", "first"]);
    let (_, t2) = tl(&url, "planner", &["task", "create", "second"]);

    let (_, started) = tl(&url, "alice", &["start", path(&a)]);
    assert_eq!(started["tasks"][0]["id"], t1["id"], "{started}");
    assert_eq!(std::fs::read_to_string(a.join("a.txt")).unwrap(), "a\n");
    std::fs::write(a.join("a.txt"), "a\nb\n").unwrap();
    std::fs::create_dir_all(a.join("pkg/__pycache__")).unwrap();
    std::fs::write(a.join("pkg/__pycache__/m.cpython.pyc"), "bytecode").unwrap();
    std::fs::write(a.join("junk.pyc"), "bytecode").unwrap();
    let (code, landed) = tl(
        &url,
        "alice",
        &["land", path(&a), "-m", "first", "--summary", "did it"],
    );
    assert_eq!(code, 0);
    assert_eq!(landed["outcome"]["outcome"], "landed");
    assert_eq!(landed["completed"][0], t1["id"], "{landed}");

    let (_, started) = tl(&url, "alice", &["start", path(&a)]);
    assert_eq!(started["tasks"][0]["id"], t2["id"], "{started}");
    tl(
        &url,
        "alice",
        &["land", path(&a), "-m", "second", "--summary", "done"],
    );
    let (_, none) = tl(&url, "alice", &["start", path(&a)]);
    assert_eq!(none["tasks"].as_array().unwrap().len(), 0, "{none}");

    tl(&url, "checker", &["open", path(&check)]);
    assert_eq!(
        std::fs::read_to_string(check.join("a.txt")).unwrap(),
        "a\nb\n"
    );
    assert!(!check.join("junk.pyc").exists());
    assert!(!check.join("pkg").exists());
    let (_, tasks) = tl(&url, "planner", &["task", "list"]);
    assert!(
        tasks
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["status"]["state"] == "done"),
        "{tasks}"
    );
}

fn server_with(max_claims: usize) -> (trellis_server::Background, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let mut config =
        trellis_server::Config::new("127.0.0.1:0".parse().unwrap(), tmp.path().join("data"));
    config.max_claims = max_claims;
    (trellis_server::start_background(config).unwrap(), tmp)
}

#[test]
fn start_can_take_several_tasks_and_land_completes_them_all() {
    let (bg, tmp) = server_with(3);
    let url = bg.url();
    let a = tmp.path().join("alice");
    tl(&url, "planner", &["init"]);
    let ids: Vec<Value> = ["one", "two", "three"]
        .iter()
        .map(|s| tl(&url, "planner", &["task", "create", s]).1["id"].clone())
        .collect();
    let (_, started) = tl(&url, "alice", &["start", path(&a), "--count", "2"]);
    let got: Vec<Value> = started["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].clone())
        .collect();
    assert_eq!(got, ids[..2].to_vec(), "{started}");
    std::fs::write(a.join("x.txt"), "x\n").unwrap();
    let (_, landed) = tl(
        &url,
        "alice",
        &["land", path(&a), "-m", "two tasks", "--summary", "done"],
    );
    assert_eq!(landed["completed"].as_array().unwrap().len(), 2, "{landed}");
    let (_, rest) = tl(&url, "alice", &["start", path(&a), "--count", "5"]);
    assert_eq!(rest["tasks"].as_array().unwrap().len(), 1, "{rest}");
    assert_eq!(rest["tasks"][0]["id"], ids[2]);
}

#[test]
fn lock_waits_for_the_holder_to_land_then_updates() {
    let (bg, tmp) = server_with(1);
    let url = bg.url();
    let (seed, a, b) = (
        tmp.path().join("seed"),
        tmp.path().join("alice"),
        tmp.path().join("bob"),
    );
    tl(&url, "planner", &["init"]);
    tl(&url, "seed", &["open", path(&seed)]);
    std::fs::write(seed.join("CHANGELOG.md"), "## Unreleased\n").unwrap();
    tl(&url, "seed", &["land", path(&seed), "-m", "seed"]);
    tl(&url, "alice", &["open", path(&a)]);
    tl(&url, "bob", &["open", path(&b)]);
    let (_, held) = tl(&url, "alice", &["lock", path(&a), "CHANGELOG.md"]);
    assert_eq!(held["outcome"], "admitted", "{held}");
    let (_, busy) = tl(&url, "bob", &["lock", path(&b), "CHANGELOG.md"]);
    assert_eq!(busy["outcome"], "conflict", "{busy}");

    let (url2, b2) = (url.clone(), b.clone());
    let waiter = std::thread::spawn(move || {
        tl(
            &url2,
            "bob",
            &[
                "lock",
                path(&b2),
                "CHANGELOG.md",
                "--wait",
                "10",
                "--update",
            ],
        )
    });
    std::thread::sleep(std::time::Duration::from_millis(600));
    std::fs::write(a.join("CHANGELOG.md"), "## Unreleased\n- alice\n").unwrap();
    tl(&url, "alice", &["land", path(&a), "-m", "alice"]);
    let (code, got) = waiter.join().unwrap();
    assert_eq!(
        (code, got["outcome"].as_str()),
        (0, Some("admitted")),
        "{got}"
    );
    assert!(got["waited_ms"].as_u64().unwrap() >= 300, "{got}");
    assert_eq!(
        std::fs::read_to_string(b.join("CHANGELOG.md")).unwrap(),
        "## Unreleased\n- alice\n"
    );
}

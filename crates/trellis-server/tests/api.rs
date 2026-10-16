//! The server and the client together, over real HTTP.

use std::collections::BTreeMap;

use trellis_client::{Client, ClientError};
use trellis_coord::{ClaimOutcome, EntryKind, NewEntry, Scope};
use trellis_land::{Intent, IntentOutcome, Outcome, RebaseOutcome};
use trellis_server::{Background, Config, start_background};

fn server() -> (Background, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let bg = start_background(Config::new(
        "127.0.0.1:0".parse().unwrap(),
        dir.path().join("data"),
    ))
    .unwrap();
    (bg, dir)
}

fn landed(outcome: &Outcome) -> trellis_core::Hash {
    match outcome {
        Outcome::Landed { changeset, .. } => *changeset,
        other => panic!("expected a landing, got {other:?}"),
    }
}

#[test]
fn files_sessions_and_landing_round_trip() {
    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    let root = c.init().unwrap();
    assert_eq!(c.main().unwrap(), root);
    let s = c.open_session("alice", Some("T-1")).unwrap();
    let odd = "docs/my notes/章节 1.md";
    c.write(&s.id, odd, b"hello").unwrap();
    c.write(&s.id, "README.md", b"# demo\n").unwrap();
    assert_eq!(c.read(&s.id, odd).unwrap().as_deref(), Some(&b"hello"[..]));
    assert_eq!(c.read(&s.id, "missing.md").unwrap(), None);
    let files = c.files(&s.id, &[]).unwrap();
    assert_eq!(files.len(), 2);
    let only_docs = c.files(&s.id, &["docs/**".into()]).unwrap();
    assert_eq!(only_docs.len(), 1);
    assert_eq!(c.blob(&only_docs[0].blob.hash).unwrap(), b"hello");

    let receipt = c.land(&s.id, "first", "k1").unwrap();
    let cs = landed(&receipt.outcome);
    assert_eq!(c.land(&s.id, "first", "k1").unwrap(), receipt);
    assert_eq!(c.main().unwrap(), cs);
    assert_eq!(c.read_main(odd).unwrap().as_deref(), Some(&b"hello"[..]));
    let history = c.changesets_since(0).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].changeset.message, "first");
    assert_eq!(history[0].changeset.provenance.task.as_deref(), Some("T-1"));
    assert_eq!(c.sessions(true).unwrap().len(), 0);
}

#[test]
fn errors_map_to_status_codes() {
    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    let not_init = c.main().unwrap_err();
    assert_eq!(not_init.code(), Some("repo-not-initialized"));
    c.init().unwrap();
    assert_eq!(c.session("nope").unwrap_err().code(), Some("not-found"));
    let s = c.open_session("a", None).unwrap();
    assert!(matches!(
        c.write(&s.id, "a\\b", b"x"),
        Err(ClientError::Api { status: 400, .. })
    ));
    c.close_session(&s.id).unwrap();
    assert_eq!(
        c.write(&s.id, "a.txt", b"x").unwrap_err().code(),
        Some("session-not-open")
    );
    let bad = Client::new(&bg.url(), "bad name").unwrap();
    assert!(matches!(
        bad.init(),
        Err(ClientError::Api { status: 400, .. })
    ));
}

#[test]
fn coordination_endpoints_work_end_to_end() {
    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();

    let task = c
        .create_task(
            "planner",
            "chapter 1",
            "write it",
            &[Scope::parse("book/ch1/**").unwrap()],
        )
        .unwrap();
    assert!(matches!(
        c.claim_task(&task.id, "alice").unwrap(),
        ClaimOutcome::Claimed(_)
    ));
    assert!(matches!(
        c.claim_task(&task.id, "bob").unwrap(),
        ClaimOutcome::Taken { .. }
    ));
    assert!(matches!(
        c.claim_next("bob").unwrap(),
        ClaimOutcome::NoneOpen
    ));
    assert_eq!(
        c.complete_task(&task.id, "bob", "x").unwrap_err().code(),
        Some("not-owner")
    );

    let alice = c.open_session("alice", Some(&task.id)).unwrap();
    let intent = Intent {
        description: "chapter 1".into(),
        writes: vec![Scope::parse("book/ch1/**").unwrap()],
        exclusive: true,
        ttl_ms: 60_000,
        ..Default::default()
    };
    assert!(matches!(
        c.declare_intent(&alice.id, &intent).unwrap(),
        IntentOutcome::Admitted { .. }
    ));
    assert_eq!(c.leases().unwrap().len(), 1);

    let watcher = c.open_session("watcher", None).unwrap();
    let watch = Intent {
        reads: vec![Scope::parse("book/**").unwrap()],
        ..Default::default()
    };
    c.declare_intent(&watcher.id, &watch).unwrap();
    c.write(&alice.id, "book/ch1/s1.md", b"scene").unwrap();
    landed(&c.land(&alice.id, "scene 1", "k").unwrap().outcome);
    c.complete_task(&task.id, "alice", "wrote scene 1").unwrap();
    let notes = c.notifications(&watcher.id).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(c.ack_notifications(&watcher.id, notes[0].seq).unwrap(), 1);
    assert!(c.notifications(&watcher.id).unwrap().is_empty());
    assert_eq!(c.all_notifications(&watcher.id).unwrap().len(), 1);
    assert!(c.leases().unwrap().is_empty());

    let fail = NewEntry {
        kind: Some(EntryKind::Fail),
        topics: vec!["ch1".into()],
        body: "timeline broken".into(),
        ..Default::default()
    };
    let entry = c.log_append("checker", &fail).unwrap();
    assert_eq!(c.log_new("alice", &[]).unwrap().len(), 1);
    assert!(c.log_new("alice", &[]).unwrap().is_empty());
    c.log_resolve(entry.seq, "alice", "fixed").unwrap();
    assert_eq!(
        c.log_entries(0, 10).unwrap()[0].resolution.as_deref(),
        Some("alice: fixed")
    );
}

#[test]
fn conflicts_are_resolved_over_http() {
    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();
    let seed = c.open_session("seed", None).unwrap();
    c.write(&seed.id, "doc.md", b"one\ntwo\nthree\n").unwrap();
    landed(&c.land(&seed.id, "seed", "k").unwrap().outcome);
    let a = c.open_session("a", None).unwrap();
    let b = c.open_session("b", None).unwrap();
    c.write(&a.id, "doc.md", b"one\nTWO-A\nthree\n").unwrap();
    c.write(&b.id, "doc.md", b"one\nTWO-B\nthree\n").unwrap();
    landed(&c.land(&a.id, "a", "k").unwrap().outcome);
    let Outcome::Conflicted { conflict } = c.land(&b.id, "b", "k").unwrap().outcome else {
        panic!("expected a conflict")
    };
    assert_eq!(c.conflicts(true).unwrap().len(), 1);
    assert!(
        c.conflict(&conflict).unwrap().paths[0]
            .detail
            .contains("<<<<<<<")
    );
    let mut resolutions = BTreeMap::new();
    resolutions.insert(
        "doc.md".to_owned(),
        Some("one\nTWO-A+B\nthree\n".to_owned()),
    );
    assert!(matches!(
        c.resolve_conflict(&conflict, &resolutions).unwrap(),
        RebaseOutcome::Rebased { .. }
    ));
    landed(&c.land(&b.id, "b", "k2").unwrap().outcome);
    assert_eq!(
        c.read_main("doc.md").unwrap().as_deref(),
        Some(&b"one\nTWO-A+B\nthree\n"[..])
    );
}

#[test]
fn notifications_stream_over_server_sent_events() {
    use std::io::Read;

    let (bg, _dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();
    let watcher = c.open_session("watcher", None).unwrap();
    c.declare_intent(
        &watcher.id,
        &Intent {
            reads: vec![Scope::All],
            ..Default::default()
        },
    )
    .unwrap();

    let url = format!("{}/repos/demo/sessions/{}/events", bg.url(), watcher.id);
    let mut stream = reqwest::blocking::Client::new().get(url).send().unwrap();
    assert!(stream.status().is_success());

    let writer = c.open_session("writer", None).unwrap();
    c.write(&writer.id, "a.md", b"x").unwrap();
    landed(&c.land(&writer.id, "write a", "k").unwrap().outcome);

    let mut seen = String::new();
    let mut buf = [0u8; 1024];
    while !seen.contains("event: notification") {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0, "stream ended before a notification arrived");
        seen.push_str(&String::from_utf8_lossy(&buf[..n]));
    }
    assert!(seen.contains("write a"), "{seen}");
}

#[test]
fn checkouts_sync_update_and_resolve() {
    use trellis_client::checkout;

    let (bg, dir) = server();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();
    let a_dir = dir.path().join("alice");
    let b_dir = dir.path().join("bob");
    let all = vec!["**".to_owned()];

    let a = c.open_session("alice", None).unwrap();
    checkout::checkout(&c, &a.id, "alice", &all, &a_dir).unwrap();
    std::fs::create_dir_all(a_dir.join("book")).unwrap();
    std::fs::write(a_dir.join("book/ch1.md"), "one\ntwo\nthree\n").unwrap();
    let (_, changes) = checkout::status(&a_dir).unwrap();
    assert_eq!(changes.written, vec!["book/ch1.md".to_owned()]);
    let synced = checkout::sync(&a_dir).unwrap();
    assert_eq!(synced.written.len(), 1);
    assert!(checkout::status(&a_dir).unwrap().1.is_empty());
    landed(&c.land(&a.id, "ch1", "k").unwrap().outcome);

    // Bob checks out after Alice landed and sees her file.
    let b = c.open_session("bob", None).unwrap();
    checkout::checkout(&c, &b.id, "bob", &all, &b_dir).unwrap();
    assert_eq!(
        std::fs::read_to_string(b_dir.join("book/ch1.md")).unwrap(),
        "one\ntwo\nthree\n"
    );

    // Alice continues in a new session; both edit different lines.
    let a2 = c.open_session("alice", None).unwrap();
    checkout::rebind(&a_dir, &a2.id).unwrap();
    std::fs::write(a_dir.join("book/ch1.md"), "ONE\ntwo\nthree\n").unwrap();
    std::fs::write(b_dir.join("book/ch1.md"), "one\ntwo\nTHREE\n").unwrap();
    checkout::sync(&a_dir).unwrap();
    landed(&c.land(&a2.id, "a", "k").unwrap().outcome);

    // Bob was notified (his checkout watches everything) and updates: merge, then refresh.
    assert_eq!(c.notifications(&b.id).unwrap().len(), 1);
    assert!(matches!(
        checkout::update(&b_dir).unwrap(),
        RebaseOutcome::Rebased { .. }
    ));
    assert_eq!(
        std::fs::read_to_string(b_dir.join("book/ch1.md")).unwrap(),
        "ONE\ntwo\nTHREE\n"
    );

    // A real conflict, resolved by editing the file with markers.
    std::fs::write(b_dir.join("book/ch1.md"), "ONE\nTWO-B\nTHREE\n").unwrap();
    let a3 = c.open_session("alice", None).unwrap();
    checkout::rebind(&a_dir, &a3.id).unwrap();
    std::fs::write(a_dir.join("book/ch1.md"), "ONE\nTWO-A\nthree\n").unwrap();
    checkout::sync(&a_dir).unwrap();
    landed(&c.land(&a3.id, "a3", "k").unwrap().outcome);
    checkout::sync(&b_dir).unwrap();
    let Outcome::Conflicted { conflict } = c.land(&b.id, "b", "k").unwrap().outcome else {
        panic!("expected a conflict")
    };
    let written = checkout::write_markers(&b_dir, &conflict).unwrap();
    assert_eq!(written, vec!["book/ch1.md".to_owned()]);
    assert!(
        std::fs::read_to_string(b_dir.join("book/ch1.md"))
            .unwrap()
            .contains("<<<<<<<")
    );
    std::fs::write(b_dir.join("book/ch1.md"), "ONE\nTWO-A+B\nthree\n").unwrap();
    assert!(matches!(
        checkout::resolve(&b_dir, &conflict).unwrap(),
        RebaseOutcome::Rebased { .. }
    ));
    landed(&c.land(&b.id, "b", "k2").unwrap().outcome);
    assert_eq!(
        c.read_main("book/ch1.md").unwrap().as_deref(),
        Some(&b"ONE\nTWO-A+B\nthree\n"[..])
    );

    // Deleting a file in the directory deletes it in the session.
    let b2 = c.open_session("bob", None).unwrap();
    checkout::rebind(&b_dir, &b2.id).unwrap();
    std::fs::remove_file(b_dir.join("book/ch1.md")).unwrap();
    assert_eq!(
        checkout::sync(&b_dir).unwrap().deleted,
        vec!["book/ch1.md".to_owned()]
    );
    landed(&c.land(&b2.id, "delete", "k").unwrap().outcome);
    assert_eq!(c.read_main("book/ch1.md").unwrap(), None);
}

#[test]
fn validators_run_after_landings_in_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new("127.0.0.1:0".parse().unwrap(), dir.path().join("data"));
    config.validators = trellis_validate::from_yaml(
        "validators:\n  - name: needs-license\n    command: [sh, -c, test -f LICENSE]\n",
    )
    .unwrap();
    let bg = start_background(config).unwrap();
    let c = Client::new(&bg.url(), "demo").unwrap();
    c.init().unwrap();
    // Let the runner record its starting position before anything lands.
    std::thread::sleep(std::time::Duration::from_millis(1500));

    let s = c.open_session("alice", None).unwrap();
    c.write(&s.id, "src/a.rs", b"fn a() {}\n").unwrap();
    landed(&c.land(&s.id, "add a", "k").unwrap().outcome);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let entry = loop {
        let fails: Vec<_> = c
            .log_entries(0, 100)
            .unwrap()
            .into_iter()
            .filter(|e| e.topics.contains(&"validator".to_owned()))
            .collect();
        if let Some(entry) = fails.into_iter().next() {
            break entry;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no validator finding appeared"
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    assert_eq!(entry.author, "validator:needs-license");
    assert!(
        c.tasks()
            .unwrap()
            .iter()
            .any(|t| t.subject.starts_with("Fix needs-license failure"))
    );
}

//! The runner against a real engine and real commands.

use std::path::Path;
use std::sync::Arc;

use trellis_coord::{EntryKind, EntryStatus, Scope};
use trellis_core::RepoPath;
use trellis_land::{Engine, Outcome, Policy, TestEnv};
use trellis_store::{MemBlobStore, SqliteMetaStore};
use trellis_validate::{Runner, Severity, ValidatorSpec};

fn engine() -> Arc<Engine> {
    let e = Engine::new(
        "r",
        Arc::new(SqliteMetaStore::open_in_memory().unwrap()),
        Arc::new(MemBlobStore::new()),
        Arc::new(TestEnv::new()),
        Policy::default(),
    );
    e.init().unwrap();
    Arc::new(e)
}

fn land(e: &Engine, principal: &str, edits: &[(&str, &str)]) -> u64 {
    let s = e.open_session(principal, None).unwrap();
    for (path, content) in edits {
        e.write(
            &s.id,
            &RepoPath::parse(path).unwrap(),
            Some(content.as_bytes()),
        )
        .unwrap();
    }
    match e
        .land(&s.id, &format!("{principal} edits"), "k")
        .unwrap()
        .outcome
    {
        Outcome::Landed { seq, .. } => seq,
        other => panic!("expected a landing, got {other:?}"),
    }
}

fn spec(name: &str, scope: &[&str], script: &str) -> ValidatorSpec {
    ValidatorSpec {
        name: name.into(),
        scope: scope.iter().map(|s| Scope::parse(s).unwrap()).collect(),
        command: vec!["sh".into(), "-c".into(), script.into()],
        timeout_seconds: 10,
        severity: Severity::Error,
    }
}

fn worktree(tmp: &tempfile::TempDir) -> std::path::PathBuf {
    tmp.path().join("worktrees").join("r")
}

#[test]
fn failures_become_fail_entries_and_repair_tasks() {
    let tmp = tempfile::tempdir().unwrap();
    let e = engine();
    land(&e, "seed", &[("README.md", "# r\n")]);
    let runner = Runner::new(
        Arc::clone(&e),
        vec![spec("needs-license", &["**"], "test -f LICENSE")],
        worktree(&tmp),
    );
    assert!(
        runner.run_pending().unwrap().is_empty(),
        "history before the first run is not validated"
    );

    let seq = land(&e, "alice", &[("src/a.rs", "fn a() {}\n")]);
    let findings = runner.run_pending().unwrap();
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(
        (finding.seq, finding.exit, finding.validator.as_str()),
        (seq, Some(1), "needs-license")
    );
    let entry = &e.coord().entries(finding.log_seq - 1, 1).unwrap()[0];
    assert_eq!(
        (entry.kind, entry.status),
        (EntryKind::Fail, EntryStatus::Open)
    );
    assert_eq!(entry.author, "validator:needs-license");
    assert_eq!(
        entry.topics,
        vec!["validator".to_owned(), "needs-license".to_owned()]
    );
    assert_eq!(entry.evidence, vec![finding.changeset.to_hex()]);
    assert!(entry.body.contains("alice"), "{}", entry.body);
    let task = e.coord().task(finding.task.as_deref().unwrap()).unwrap();
    assert_eq!(
        task.subject,
        format!("Fix needs-license failure from seq {seq}")
    );
    assert_eq!(task.scope, vec![Scope::parse("src/a.rs").unwrap()]);

    land(&e, "bob", &[("LICENSE", "MIT\n")]);
    assert!(runner.run_pending().unwrap().is_empty());
    for path in ["README.md", "src/a.rs", "LICENSE"] {
        assert!(
            worktree(&tmp).join(path).exists(),
            "{path} missing from the worktree"
        );
    }
}

#[test]
fn validators_run_only_for_their_scope_and_see_the_landed_tree_and_environment() {
    let tmp = tempfile::tempdir().unwrap();
    let e = engine();
    let checks = vec![
        spec("src-ok", &["src/**"], "grep -qx ok src/a.rs"),
        spec(
            "env",
            &["src/**"],
            r#"test "$TRELLIS_PRINCIPAL" = carol && test "$TRELLIS_CHANGED" = src/a.rs && test -n "$TRELLIS_CHANGESET""#,
        ),
    ];
    let runner = Runner::new(Arc::clone(&e), checks, worktree(&tmp));
    runner.run_pending().unwrap();
    land(&e, "dave", &[("docs/x.md", "anything\n")]);
    assert!(
        runner.run_pending().unwrap().is_empty(),
        "docs changes do not trigger src validators"
    );
    land(&e, "carol", &[("src/a.rs", "bad\n")]);
    let names: Vec<String> = runner
        .run_pending()
        .unwrap()
        .into_iter()
        .map(|f| f.validator)
        .collect();
    assert_eq!(names, ["src-ok"]);
    land(&e, "carol", &[("src/a.rs", "ok\n")]);
    assert!(runner.run_pending().unwrap().is_empty());
}

#[test]
fn timeouts_and_warnings_are_findings_without_tasks() {
    let tmp = tempfile::tempdir().unwrap();
    let e = engine();
    let mut slow = spec("slow", &["**"], "sleep 5");
    slow.timeout_seconds = 1;
    slow.severity = Severity::Warning;
    let runner = Runner::new(Arc::clone(&e), vec![slow], worktree(&tmp));
    runner.run_pending().unwrap();
    land(&e, "a", &[("x.md", "x\n")]);
    let findings = runner.run_pending().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].exit, None);
    assert!(findings[0].output.contains("timed out after 1s"));
    assert_eq!(findings[0].task, None);
    assert_eq!(e.coord().entries(0, 10).unwrap()[0].kind, EntryKind::Note);
    assert!(e.coord().tasks().unwrap().is_empty());
}

#[test]
fn a_new_runner_resumes_from_the_recorded_position() {
    let tmp = tempfile::tempdir().unwrap();
    let e = engine();
    let checks = || vec![spec("never", &["**"], "false")];
    Runner::new(Arc::clone(&e), checks(), worktree(&tmp))
        .run_pending()
        .unwrap();
    land(&e, "a", &[("one.md", "1\n")]);
    assert_eq!(
        Runner::new(Arc::clone(&e), checks(), worktree(&tmp))
            .run_pending()
            .unwrap()
            .len(),
        1
    );
    land(&e, "a", &[("two.md", "2\n")]);
    let again = Runner::new(Arc::clone(&e), checks(), worktree(&tmp))
        .run_pending()
        .unwrap();
    assert_eq!(again.len(), 1, "only the new landing is validated");
    assert!(Path::new(&worktree(&tmp)).join("one.md").exists());
}

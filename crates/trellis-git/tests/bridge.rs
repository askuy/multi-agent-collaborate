//! Import from and export to a real Git repository through a real Trellis server.

use std::path::Path;
use std::process::Command;

use trellis_client::Client;
use trellis_git::{GitError, export, import};
use trellis_land::Outcome;
use trellis_server::{Background, Config, start_background};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Human",
            "-c",
            "user.email=human@example.com",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn setup() -> (Background, Client, tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let bg = start_background(Config::new(
        "127.0.0.1:0".parse().unwrap(),
        tmp.path().join("data"),
    ))
    .unwrap();
    let client = Client::new(&bg.url(), "demo").unwrap();
    client.init().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("README.md"), "# demo\n").unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn one() -> u8 { 1 }\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "initial"]);
    (bg, client, tmp, repo)
}

fn land(client: &Client, principal: &str, edits: &[(&str, Option<&str>)], message: &str) {
    let s = client.open_session(principal, Some("T-7")).unwrap();
    for (path, content) in edits {
        match content {
            Some(text) => client.write(&s.id, path, text.as_bytes()).unwrap(),
            None => client.delete(&s.id, path).unwrap(),
        }
    }
    assert!(matches!(
        client.land(&s.id, message, "k").unwrap().outcome,
        Outcome::Landed { .. }
    ));
}

#[test]
fn import_then_export_round_trips_through_git() {
    let (_bg, c, _tmp, repo) = setup();
    let report = import(&c, "importer", &repo, "HEAD").unwrap();
    assert_eq!((report.written, report.deleted), (2, 0));
    assert_eq!(
        c.read_main("src/lib.rs").unwrap().as_deref(),
        Some(&b"pub fn one() -> u8 { 1 }\n"[..])
    );
    // The import itself is not exported again.
    assert!(export(&c, &repo).unwrap().commits.is_empty());

    land(
        &c,
        "alice",
        &[
            ("src/lib.rs", Some("pub fn one() -> u8 { 2 }\n")),
            ("README.md", None),
            ("docs/new.md", Some("new\n")),
        ],
        "alice's change",
    );
    let exported = export(&c, &repo).unwrap();
    assert_eq!(exported.commits.len(), 1);
    assert_eq!(
        std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
        "pub fn one() -> u8 { 2 }\n"
    );
    assert!(!repo.join("README.md").exists());
    assert_eq!(
        std::fs::read_to_string(repo.join("docs/new.md")).unwrap(),
        "new\n"
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%an <%ae>"]),
        "alice <alice@trellis.invalid>"
    );
    let body = git(&repo, &["log", "-1", "--format=%B"]);
    assert!(body.starts_with("alice's change"), "{body}");
    assert!(
        body.contains(&format!(
            "Trellis-Changeset: {}",
            exported.commits[0].changeset
        )),
        "{body}"
    );
    assert!(body.contains("Trellis-Task: T-7"), "{body}");
    assert!(git(&repo, &["status", "--porcelain"]).is_empty());
    assert!(export(&c, &repo).unwrap().commits.is_empty());

    // A human commits in Git; importing brings it into Trellis without re-exporting it.
    std::fs::write(repo.join("docs/new.md"), "edited by a human\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "human edit"]);
    let second = import(&c, "importer", &repo, "HEAD").unwrap();
    assert_eq!((second.written, second.deleted), (1, 0));
    assert_eq!(
        c.read_main("docs/new.md").unwrap().as_deref(),
        Some(&b"edited by a human\n"[..])
    );
    assert!(export(&c, &repo).unwrap().commits.is_empty());
}

#[test]
fn import_refuses_to_skip_unexported_changes() {
    let (_bg, c, _tmp, repo) = setup();
    import(&c, "importer", &repo, "HEAD").unwrap();
    land(&c, "alice", &[("x.md", Some("x\n"))], "x");
    let err = import(&c, "importer", &repo, "HEAD").unwrap_err();
    assert!(
        matches!(err, GitError::Invalid(ref m) if m.contains("export first")),
        "{err}"
    );
    assert_eq!(export(&c, &repo).unwrap().commits.len(), 1);
    import(&c, "importer", &repo, "HEAD").unwrap();
}

#[test]
fn export_refuses_a_dirty_worktree() {
    let (_bg, c, _tmp, repo) = setup();
    import(&c, "importer", &repo, "HEAD").unwrap();
    land(&c, "alice", &[("y.md", Some("y\n"))], "y");
    std::fs::write(repo.join("scratch.txt"), "uncommitted").unwrap();
    let err = export(&c, &repo).unwrap_err();
    assert!(
        matches!(err, GitError::Invalid(ref m) if m.contains("uncommitted")),
        "{err}"
    );
}

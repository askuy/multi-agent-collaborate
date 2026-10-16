//! Many threads land at once on one engine backed by a real SQLite file. Each session writes
//! its own file plus its own key in a shared JSON document, so every landing must succeed
//! (merges are field-level) even though most of them lose the CAS at least once.

use std::collections::BTreeSet;
use std::sync::Arc;

use trellis_core::RepoPath;
use trellis_derive::{DeriveCtx, SkipList};
use trellis_land::{Engine, Outcome, Policy, Route, SystemEnv, WriteMode};
use trellis_store::{FsBlobStore, MetaStore, SqliteMetaStore};

const THREADS: usize = 8;
const SESSIONS_PER_THREAD: usize = 12;

#[test]
fn concurrent_landings_all_succeed_and_form_one_chain() {
    let dir = tempfile::tempdir().unwrap();
    let meta: Arc<dyn MetaStore> =
        Arc::new(SqliteMetaStore::open(dir.path().join("meta.db")).unwrap());
    let blobs = Arc::new(FsBlobStore::open(dir.path().join("objects")).unwrap());
    let policy = Policy::new(&[("serial/**", WriteMode::Serialize)]).unwrap();
    let engine = Arc::new(Engine::new(
        "repo",
        Arc::clone(&meta),
        blobs.clone(),
        Arc::new(SystemEnv::new()),
        policy,
    ));
    let root = engine.init().unwrap();
    let shared = RepoPath::parse("canon/shared.json").unwrap();

    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let (engine, shared) = (Arc::clone(&engine), shared.clone());
            std::thread::spawn(move || {
                let mut landed = Vec::new();
                for i in 0..SESSIONS_PER_THREAD {
                    let s = engine.open_session(&format!("agent-{t}"), None).unwrap();
                    let current = engine.read(&s.id, &shared).unwrap();
                    let mut doc: serde_json::Map<String, serde_json::Value> = current
                        .map(|b| serde_json::from_slice(&b).unwrap())
                        .unwrap_or_default();
                    doc.insert(format!("t{t}-{i}"), serde_json::json!(i));
                    engine
                        .write(&s.id, &shared, Some(&serde_json::to_vec(&doc).unwrap()))
                        .unwrap();
                    let own = RepoPath::parse(&format!("agents/{t}/{i}.txt")).unwrap();
                    engine
                        .write(&s.id, &own, Some(format!("{t}/{i}").as_bytes()))
                        .unwrap();
                    if i % 4 == 0 {
                        let serial = RepoPath::parse(&format!("serial/{t}-{i}.txt")).unwrap();
                        engine.write(&s.id, &serial, Some(b"s")).unwrap();
                    }
                    let receipt = engine.land(&s.id, "work", "k").unwrap();
                    match receipt.outcome {
                        Outcome::Landed {
                            changeset, route, ..
                        } => {
                            if i % 4 == 0 {
                                assert_eq!(route, Route::Serialized);
                            }
                            landed.push(changeset);
                        }
                        other => panic!("thread {t} session {i}: {other:?}"),
                    }
                }
                landed
            })
        })
        .collect();
    let landed: BTreeSet<_> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert_eq!(landed.len(), THREADS * SESSIONS_PER_THREAD);

    // Invariant 8: main is one first-parent chain whose generations count every landing.
    let main = engine.main().unwrap();
    let ctx = DeriveCtx::new("repo", meta.as_ref(), blobs.as_ref());
    assert_eq!(
        ctx.get::<SkipList>(&main).unwrap().generation as usize,
        landed.len()
    );
    let mut chain = BTreeSet::new();
    let mut current = main;
    while current != root {
        chain.insert(current);
        current = ctx.changeset(&current).unwrap().first_parent().unwrap();
    }
    // Invariant 1: every acknowledged landing is on main.
    assert_eq!(chain, landed);

    // Every field-level JSON merge kept every key.
    let doc: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&engine.read_main(&shared).unwrap().unwrap()).unwrap();
    assert_eq!(doc.len(), THREADS * SESSIONS_PER_THREAD);
}

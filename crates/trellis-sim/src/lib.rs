//! Deterministic simulation (spec §8): many virtual agents edit, land, rebase, resolve
//! conflicts, abandon sessions, and repeat requests against one engine, under injected
//! faults. Everything is driven by one seed, so a failure replays exactly.

mod agent;
mod check;
mod faulty;

use std::collections::BTreeMap;
use std::sync::Arc;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use trellis_core::Hash;
use trellis_derive::DeriveCtx;
use trellis_land::{Engine, Outcome, Policy, TestEnv, WriteMode};
use trellis_store::{MemBlobStore, MetaStore, SqliteMetaStore};

pub use agent::{Agent, Event, UNIVERSE};
pub use faulty::FaultyMeta;

#[derive(Clone, Debug)]
pub struct SimConfig {
    pub seed: u64,
    pub agents: usize,
    pub steps: usize,
    /// Probability that a ref CAS spuriously fails.
    pub cas_fault: f64,
    /// Probability that writing a receipt fails after a landing.
    pub receipt_fault: f64,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SimReport {
    pub main: Option<Hash>,
    pub landed: usize,
    pub unchanged: usize,
    pub conflicted: usize,
    pub rejected: usize,
    pub errors: usize,
    pub tasks_done: usize,
    pub notifications: usize,
    pub log_entries: u64,
}

pub fn run(config: &SimConfig) -> Result<SimReport, String> {
    let meta: Arc<dyn MetaStore> = Arc::new(FaultyMeta::new(
        SqliteMetaStore::open_in_memory().map_err(|e| e.to_string())?,
        config.seed ^ 0x5eed,
        config.cas_fault,
        config.receipt_fault,
    ));
    let blobs = Arc::new(MemBlobStore::new());
    let policy = Policy::new(&[
        ("strict/**", WriteMode::Strict),
        ("serial/**", WriteMode::Serialize),
    ])
    .map_err(|e| e.to_string())?;
    let engine = Engine::new(
        "sim",
        Arc::clone(&meta),
        blobs.clone(),
        Arc::new(TestEnv::new()),
        policy,
    );
    let root = engine.init().map_err(|e| e.to_string())?;
    for i in 0..24 {
        engine
            .coord()
            .create_task(
                "planner",
                &format!("task {i}"),
                "simulated work",
                Vec::new(),
            )
            .map_err(|e| e.to_string())?;
    }
    let ctx = DeriveCtx::new("sim", meta.as_ref(), blobs.as_ref());

    let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
    let mut agents: Vec<Agent> = (0..config.agents)
        .map(|i| Agent {
            name: format!("agent-{i}"),
            ..Default::default()
        })
        .collect();
    let mut receipts: BTreeMap<(String, String), trellis_land::Receipt> = BTreeMap::new();
    let mut mismatched = 0;
    let mut report = SimReport::default();

    for _ in 0..config.steps {
        let i = rng.random_range(0..agents.len());
        match agents[i].step(&engine, &mut rng) {
            Event::Nothing => {}
            Event::Error => report.errors += 1,
            Event::Read { previous, seqs } => check::check_read(previous, &seqs)?,
            Event::Receipt { receipt, first_try } => {
                let key = (receipt.session.clone(), receipt.idempotency_key.clone());
                match receipts.get(&key) {
                    Some(first) if *first != receipt => mismatched += 1,
                    Some(_) => {}
                    None => {
                        match &receipt.outcome {
                            Outcome::Landed { .. } => report.landed += 1,
                            Outcome::Unchanged { .. } => report.unchanged += 1,
                            Outcome::Conflicted { .. } => report.conflicted += 1,
                            Outcome::Rejected { .. } => report.rejected += 1,
                        }
                        if first_try {
                            check::check_notified(&engine, &ctx, &receipt)?;
                        }
                        receipts.insert(key, receipt);
                    }
                }
            }
        }
        check::check_leases(&engine)?;
        check::check_tasks(&engine)?;
    }
    // Drain: retry every request that failed with an injected fault, so that every landing
    // on main has been acknowledged before checking.
    for agent in &mut agents {
        for _ in 0..64 {
            let (Some(session), Some(key)) = (agent.session.clone(), agent.pending_key.clone())
            else {
                break;
            };
            if let Ok(receipt) = engine.land(&session, "sim", &key) {
                agent.pending_key = None;
                if let Outcome::Landed { .. } = receipt.outcome {
                    report.landed += 1;
                }
                receipts.insert(
                    (receipt.session.clone(), receipt.idempotency_key.clone()),
                    receipt,
                );
            }
        }
    }

    check::check(&engine, &ctx, root, &receipts, mismatched)?;
    report.tasks_done = engine
        .coord()
        .tasks()
        .map_err(|e| e.to_string())?
        .iter()
        .filter(|t| matches!(t.status, trellis_coord::TaskStatus::Done { .. }))
        .count();
    for session in engine.sessions(false).map_err(|e| e.to_string())? {
        report.notifications += engine
            .all_notifications(&session.id)
            .map_err(|e| e.to_string())?
            .len();
    }
    report.log_entries = engine
        .coord()
        .entries(0, usize::MAX)
        .map_err(|e| e.to_string())?
        .len() as u64;
    report.main = Some(engine.main().map_err(|e| e.to_string())?);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(seed: u64) -> SimConfig {
        SimConfig {
            seed,
            agents: 12,
            steps: 1500,
            cas_fault: 0.15,
            receipt_fault: 0.05,
        }
    }

    #[test]
    fn invariants_hold_across_seeds_and_every_route_is_exercised() {
        let mut total = SimReport::default();
        for seed in 0..12 {
            let report = run(&config(seed)).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            total.landed += report.landed;
            total.conflicted += report.conflicted;
            total.rejected += report.rejected;
            total.errors += report.errors;
            total.tasks_done += report.tasks_done;
            total.notifications += report.notifications;
        }
        assert!(total.landed > 100, "{total:?}");
        assert!(total.conflicted > 0, "{total:?}");
        assert!(total.rejected > 0, "{total:?}");
        assert!(total.errors > 0, "faults were injected: {total:?}");
        assert!(total.tasks_done > 0, "{total:?}");
        assert!(total.notifications > 0, "{total:?}");
    }

    #[test]
    fn a_seed_replays_exactly() {
        assert_eq!(run(&config(7)).unwrap(), run(&config(7)).unwrap());
    }
}

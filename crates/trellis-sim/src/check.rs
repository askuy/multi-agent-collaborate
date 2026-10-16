use std::collections::{BTreeMap, BTreeSet};

use trellis_coord::{LeaseMode, TaskStatus};
use trellis_core::Hash;
use trellis_derive::{DeriveCtx, SkipList, is_first_parent_ancestor};
use trellis_land::{Engine, Outcome, Receipt};

/// Checks spec §7 invariants 1, 3, 6, and 8 over everything the agents saw.
pub fn check(
    engine: &Engine,
    ctx: &DeriveCtx<'_>,
    root: Hash,
    receipts: &BTreeMap<(String, String), Receipt>,
    duplicates_mismatched: usize,
) -> Result<usize, String> {
    // Invariant 3: a repeated request returned exactly the first receipt.
    if duplicates_mismatched > 0 {
        return Err(format!(
            "{duplicates_mismatched} retried requests returned a different receipt"
        ));
    }
    // Invariant 8: main is one first-parent chain with generations 0..n.
    let main = engine.main().map_err(|e| e.to_string())?;
    let mut chain = Vec::new();
    let mut current = main;
    loop {
        let generation = ctx
            .get::<SkipList>(&current)
            .map_err(|e| e.to_string())?
            .generation;
        let cs = ctx.changeset(&current).map_err(|e| e.to_string())?;
        chain.push((current, generation, cs.provenance.session.clone()));
        match cs.first_parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    if current != root {
        return Err("main does not descend from the root".into());
    }
    for (i, (_, generation, _)) in chain.iter().rev().enumerate() {
        if *generation != i as u64 {
            return Err(format!("generation {generation} at chain position {i}"));
        }
    }
    // No session landed twice.
    let mut sessions = BTreeSet::new();
    for (_, _, session) in &chain {
        if let Some(session) = session {
            if !sessions.insert(session.clone()) {
                return Err(format!("session {session} landed twice"));
            }
        }
    }
    let on_main: BTreeSet<Hash> = chain.iter().map(|(h, _, _)| *h).collect();
    let mut landed = 0;
    for receipt in receipts.values() {
        match &receipt.outcome {
            // Invariant 1: an acknowledged landing stays on main.
            Outcome::Landed { changeset, .. } => {
                if !on_main.contains(changeset) {
                    return Err(format!("landed changeset {changeset} is not on main"));
                }
                landed += 1;
            }
            // Invariant 6: a conflicted receipt points at a stored conflict.
            Outcome::Conflicted { conflict } => {
                engine
                    .conflict(conflict)
                    .map_err(|e| format!("conflict {conflict}: {e}"))?;
            }
            Outcome::Unchanged { .. } | Outcome::Rejected { .. } => {}
        }
    }
    // Every landing on main was acknowledged to its agent (possibly through a retry).
    if landed != chain.len() - 1 {
        return Err(format!(
            "{} changesets on main but {landed} landed receipts",
            chain.len() - 1
        ));
    }
    Ok(landed)
}

/// Invariant 2: no two active leases of different holders overlap unless both are shared.
pub fn check_leases(engine: &Engine) -> Result<(), String> {
    let leases = engine.coord().active_leases().map_err(|e| e.to_string())?;
    for (i, a) in leases.iter().enumerate() {
        for b in &leases[i + 1..] {
            let exclusive = a.mode == LeaseMode::Exclusive || b.mode == LeaseMode::Exclusive;
            if a.holder != b.holder && exclusive && a.scope.overlaps(&b.scope) {
                return Err(format!(
                    "leases {} ({}) and {} ({}) overlap",
                    a.id, a.holder, b.id, b.holder
                ));
            }
        }
    }
    Ok(())
}

/// Invariant 4: no principal holds more than one unfinished claim.
pub fn check_tasks(engine: &Engine) -> Result<(), String> {
    let mut owners = BTreeSet::new();
    for task in engine.coord().tasks().map_err(|e| e.to_string())? {
        if let TaskStatus::Claimed { owner, .. } = task.status {
            if !owners.insert(owner.clone()) {
                return Err(format!("{owner} holds more than one claim"));
            }
        }
    }
    Ok(())
}

/// Invariant 5: a landing notified every open session that read or watches a path it
/// changed and does not include it yet.
pub fn check_notified(
    engine: &Engine,
    ctx: &DeriveCtx<'_>,
    receipt: &Receipt,
) -> Result<(), String> {
    let Outcome::Landed { changeset, .. } = &receipt.outcome else {
        return Ok(());
    };
    let cs = ctx.changeset(changeset).map_err(|e| e.to_string())?;
    for session in engine.sessions(true).map_err(|e| e.to_string())? {
        if session.id == receipt.session
            || is_first_parent_ancestor(ctx, changeset, &session.base).map_err(|e| e.to_string())?
        {
            continue;
        }
        let affected = cs
            .changes
            .keys()
            .any(|p| session.reads.contains(p) || session.watch.iter().any(|w| w.covers(p)));
        let notified = engine
            .all_notifications(&session.id)
            .map_err(|e| e.to_string())?
            .iter()
            .any(|n| n.changeset == *changeset);
        if affected && !notified {
            return Err(format!(
                "session {} was not notified of {changeset}",
                session.id
            ));
        }
    }
    Ok(())
}

/// A log read returns ascending entries, all newer than anything that reader saw before.
pub fn check_read(previous: u64, seqs: &[u64]) -> Result<(), String> {
    if seqs.windows(2).any(|w| w[0] >= w[1]) || seqs.first().is_some_and(|first| *first <= previous)
    {
        return Err(format!("log read after {previous} returned {seqs:?}"));
    }
    Ok(())
}

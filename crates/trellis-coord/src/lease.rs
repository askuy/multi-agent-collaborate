use serde::{Deserialize, Serialize};
use trellis_core::RepoPath;

use crate::coord::Coord;
use crate::error::{CoordError, Result};
use crate::scope::Scope;

pub(crate) const LEASE: &str = "lease";
const LEASES_LOCK: &str = "leases";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LeaseMode {
    Exclusive,
    Shared,
}

/// A claim on a scope until `expires_at_ms` (spec §2, AgentRoom claims, Agent Mail leases).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Lease {
    pub id: String,
    pub holder: String,
    pub scope: Scope,
    pub mode: LeaseMode,
    pub intent: String,
    pub expires_at_ms: i64,
}

/// Who is in the way, what for, and until when.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LeaseConflict {
    pub holder: String,
    pub scope: Scope,
    pub mode: LeaseMode,
    pub intent: String,
    pub expires_at_ms: i64,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Acquire {
    Granted(Vec<Lease>),
    Conflict(LeaseConflict),
}

impl From<&Lease> for LeaseConflict {
    fn from(lease: &Lease) -> Self {
        Self {
            holder: lease.holder.clone(),
            scope: lease.scope.clone(),
            mode: lease.mode,
            intent: lease.intent.clone(),
            expires_at_ms: lease.expires_at_ms,
        }
    }
}

fn clashes(existing: &Lease, holder: &str, scope: &Scope, mode: LeaseMode) -> bool {
    existing.holder != holder
        && existing.scope.overlaps(scope)
        && (existing.mode == LeaseMode::Exclusive || mode == LeaseMode::Exclusive)
}

impl Coord {
    /// Grants a lease per scope, all or nothing; a conflict names the blocking lease.
    /// Expired leases are pruned on the way.
    pub fn acquire_leases(
        &self,
        holder: &str,
        scopes: &[Scope],
        mode: LeaseMode,
        ttl_ms: i64,
        intent: &str,
    ) -> Result<Acquire> {
        let _guard = self.locks.lock(&[LEASES_LOCK.to_owned()]);
        let now = self.env.now_ms();
        let mut active = Vec::new();
        for lease in self.load_all::<Lease>(LEASE)? {
            if lease.expires_at_ms > now {
                active.push(lease);
            } else {
                self.delete(LEASE, &lease.id)?;
            }
        }
        for scope in scopes {
            if let Some(blocking) = active.iter().find(|l| clashes(l, holder, scope, mode)) {
                return Ok(Acquire::Conflict(blocking.into()));
            }
        }
        let leases: Vec<Lease> = scopes
            .iter()
            .map(|scope| Lease {
                id: self.env.new_id("l"),
                holder: holder.to_owned(),
                scope: scope.clone(),
                mode,
                intent: intent.to_owned(),
                expires_at_ms: now + ttl_ms,
            })
            .collect();
        for lease in &leases {
            self.save(LEASE, &lease.id, lease)?;
        }
        Ok(Acquire::Granted(leases))
    }

    pub fn renew_lease(&self, id: &str, holder: &str, ttl_ms: i64) -> Result<Lease> {
        let _guard = self.locks.lock(&[LEASES_LOCK.to_owned()]);
        let now = self.env.now_ms();
        let mut lease: Lease = self
            .load(LEASE, id)?
            .filter(|l: &Lease| l.expires_at_ms > now)
            .ok_or_else(|| CoordError::LeaseNotFound(id.to_owned()))?;
        if lease.holder != holder {
            return Err(CoordError::NotLeaseHolder {
                lease: id.to_owned(),
                principal: holder.to_owned(),
            });
        }
        lease.expires_at_ms = now + ttl_ms;
        self.save(LEASE, id, &lease)?;
        Ok(lease)
    }

    /// Releases a lease; releasing one that is gone is not an error.
    pub fn release_lease(&self, id: &str, holder: &str) -> Result<()> {
        let _guard = self.locks.lock(&[LEASES_LOCK.to_owned()]);
        match self.load::<Lease>(LEASE, id)? {
            None => Ok(()),
            Some(lease) if lease.holder != holder => Err(CoordError::NotLeaseHolder {
                lease: id.to_owned(),
                principal: holder.to_owned(),
            }),
            Some(_) => self.delete(LEASE, id),
        }
    }

    pub fn active_leases(&self) -> Result<Vec<Lease>> {
        let now = self.env.now_ms();
        Ok(self
            .load_all::<Lease>(LEASE)?
            .into_iter()
            .filter(|l| l.expires_at_ms > now)
            .collect())
    }

    /// The active exclusive lease, held by someone other than `principal`, that covers `path`.
    pub fn write_blocker(&self, path: &RepoPath, principal: &str) -> Result<Option<LeaseConflict>> {
        Ok(self
            .active_leases()?
            .iter()
            .find(|l| {
                l.holder != principal && l.mode == LeaseMode::Exclusive && l.scope.covers(path)
            })
            .map(LeaseConflict::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::tests::coord;

    fn s(x: &str) -> Scope {
        Scope::parse(x).unwrap()
    }

    fn rp(x: &str) -> RepoPath {
        RepoPath::parse(x).unwrap()
    }

    fn granted(a: Acquire) -> Vec<Lease> {
        match a {
            Acquire::Granted(leases) => leases,
            Acquire::Conflict(c) => panic!("expected a grant, got {c:?}"),
        }
    }

    #[test]
    fn exclusive_leases_exclude_overlapping_scopes_of_other_holders() {
        let (c, _) = coord();
        let held = granted(
            c.acquire_leases("a", &[s("book/ch1/**")], LeaseMode::Exclusive, 100, "ch1")
                .unwrap(),
        );
        let Acquire::Conflict(conflict) = c
            .acquire_leases("b", &[s("book/ch1/s1.md")], LeaseMode::Shared, 100, "peek")
            .unwrap()
        else {
            panic!("expected a conflict")
        };
        assert_eq!(
            (conflict.holder.as_str(), conflict.intent.as_str()),
            ("a", "ch1")
        );
        granted(
            c.acquire_leases("b", &[s("book/ch2/**")], LeaseMode::Exclusive, 100, "ch2")
                .unwrap(),
        );

        // All or nothing: one blocked scope grants none.
        let all = c
            .acquire_leases(
                "c",
                &[s("free.md"), s("book/**")],
                LeaseMode::Exclusive,
                100,
                "x",
            )
            .unwrap();
        assert!(matches!(all, Acquire::Conflict(_)));
        assert!(c.active_leases().unwrap().iter().all(|l| l.holder != "c"));

        assert_eq!(
            c.write_blocker(&rp("book/ch1/s1.md"), "b")
                .unwrap()
                .unwrap()
                .holder,
            "a"
        );
        assert_eq!(c.write_blocker(&rp("book/ch1/s1.md"), "a").unwrap(), None);
        assert_eq!(c.write_blocker(&rp("elsewhere.md"), "b").unwrap(), None);

        c.release_lease(&held[0].id, "a").unwrap();
        assert_eq!(c.write_blocker(&rp("book/ch1/s1.md"), "b").unwrap(), None);
    }

    #[test]
    fn shared_leases_coexist_and_a_holder_never_blocks_itself() {
        let (c, _) = coord();
        granted(
            c.acquire_leases("a", &[s("notes/**")], LeaseMode::Shared, 100, "read")
                .unwrap(),
        );
        granted(
            c.acquire_leases("b", &[s("notes/x.md")], LeaseMode::Shared, 100, "read")
                .unwrap(),
        );
        granted(
            c.acquire_leases("a", &[s("notes/x.md")], LeaseMode::Shared, 100, "more")
                .unwrap(),
        );
        let excl = c
            .acquire_leases("b", &[s("notes/**")], LeaseMode::Exclusive, 100, "w")
            .unwrap();
        assert!(matches!(excl, Acquire::Conflict(_)));
        granted(
            c.acquire_leases("z", &[s("solo/**")], LeaseMode::Exclusive, 100, "w")
                .unwrap(),
        );
        granted(
            c.acquire_leases("z", &[s("solo/a.md")], LeaseMode::Exclusive, 100, "w2")
                .unwrap(),
        );
    }

    #[test]
    fn leases_expire_and_can_be_renewed_by_their_holder() {
        let (c, env) = coord();
        let held = granted(
            c.acquire_leases("a", &[s("x")], LeaseMode::Exclusive, 50, "w")
                .unwrap(),
        );
        env.advance(30);
        let renewed = c.renew_lease(&held[0].id, "a", 50).unwrap();
        assert!(renewed.expires_at_ms > held[0].expires_at_ms);
        assert!(matches!(
            c.renew_lease(&held[0].id, "b", 50),
            Err(CoordError::NotLeaseHolder { .. })
        ));
        assert!(matches!(
            c.release_lease(&held[0].id, "b"),
            Err(CoordError::NotLeaseHolder { .. })
        ));
        env.advance(1000);
        assert!(c.active_leases().unwrap().is_empty());
        assert!(matches!(
            c.renew_lease(&held[0].id, "a", 50),
            Err(CoordError::LeaseNotFound(_))
        ));
        granted(
            c.acquire_leases("b", &[s("x")], LeaseMode::Exclusive, 50, "after")
                .unwrap(),
        );
        c.release_lease("never-existed", "b").unwrap();
    }
}

use serde::{Deserialize, Serialize};

use crate::coord::Coord;
use crate::error::{CoordError, Result};
use crate::scope::Scope;

pub(crate) const TASK: &str = "task";
const TASKS_LOCK: &str = "tasks";

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum TaskStatus {
    Open,
    Claimed { owner: String, at_ms: i64 },
    Done { owner: String, summary: String },
}

/// One unit of work in the shared queue (spec §2, DeLM task queue).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    /// Intended output, inputs, and the interface shared with other work (DeLM's guidance).
    pub detail: String,
    pub created_by: String,
    pub scope: Vec<Scope>,
    pub status: TaskStatus,
    pub created_at_ms: i64,
}

/// The deterministic answer to a claim (AgentRoom: claims return a conflict, never a guess).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum ClaimOutcome {
    Claimed(Task),
    Taken {
        owner: String,
    },
    AtLimit {
        holding: Vec<String>,
    },
    Finished,
    /// `claim_next` found no open task.
    NoneOpen,
}

impl Coord {
    pub fn create_task(
        &self,
        created_by: &str,
        subject: &str,
        detail: &str,
        scope: Vec<Scope>,
    ) -> Result<Task> {
        let task = Task {
            id: self.env.new_id("t"),
            subject: subject.to_owned(),
            detail: detail.to_owned(),
            created_by: created_by.to_owned(),
            scope,
            status: TaskStatus::Open,
            created_at_ms: self.env.now_ms(),
        };
        self.save(TASK, &task.id, &task)?;
        Ok(task)
    }

    pub fn task(&self, id: &str) -> Result<Task> {
        self.load(TASK, id)?
            .ok_or_else(|| CoordError::TaskNotFound(id.to_owned()))
    }

    /// Every task, oldest first.
    pub fn tasks(&self) -> Result<Vec<Task>> {
        let mut tasks: Vec<Task> = self.load_all(TASK)?;
        tasks.sort_by(|a, b| (a.created_at_ms, &a.id).cmp(&(b.created_at_ms, &b.id)));
        Ok(tasks)
    }

    /// The first claim wins; a principal holds at most `max_claims` unfinished claims.
    /// Re-claiming a task one already owns is idempotent.
    pub fn claim_task(&self, id: &str, principal: &str) -> Result<ClaimOutcome> {
        let _guard = self.locks.lock(&[TASKS_LOCK.to_owned()]);
        let mut task = self.task(id)?;
        match &task.status {
            TaskStatus::Done { .. } => return Ok(ClaimOutcome::Finished),
            TaskStatus::Claimed { owner, .. } if owner == principal => {
                return Ok(ClaimOutcome::Claimed(task));
            }
            TaskStatus::Claimed { owner, .. } => {
                return Ok(ClaimOutcome::Taken {
                    owner: owner.clone(),
                });
            }
            TaskStatus::Open => {}
        }
        let holding: Vec<String> = self
            .tasks()?
            .into_iter()
            .filter(
                |t| matches!(&t.status, TaskStatus::Claimed { owner, .. } if owner == principal),
            )
            .map(|t| t.id)
            .collect();
        if holding.len() >= self.max_claims {
            return Ok(ClaimOutcome::AtLimit { holding });
        }
        task.status = TaskStatus::Claimed {
            owner: principal.to_owned(),
            at_ms: self.env.now_ms(),
        };
        self.save(TASK, &task.id, &task)?;
        Ok(ClaimOutcome::Claimed(task))
    }

    /// Claims the oldest open task for `principal` in one step, so agents racing for work never
    /// pick the same task and retry (a `task_list` then `task_claim` round per lost race).
    pub fn claim_next(&self, principal: &str) -> Result<ClaimOutcome> {
        let _guard = self.locks.lock(&[TASKS_LOCK.to_owned()]);
        let tasks = self.tasks()?;
        let holding: Vec<String> = tasks
            .iter()
            .filter(
                |t| matches!(&t.status, TaskStatus::Claimed { owner, .. } if owner == principal),
            )
            .map(|t| t.id.clone())
            .collect();
        if holding.len() >= self.max_claims {
            return Ok(ClaimOutcome::AtLimit { holding });
        }
        let Some(mut task) = tasks.into_iter().find(|t| t.status == TaskStatus::Open) else {
            return Ok(ClaimOutcome::NoneOpen);
        };
        task.status = TaskStatus::Claimed {
            owner: principal.to_owned(),
            at_ms: self.env.now_ms(),
        };
        self.save(TASK, &task.id, &task)?;
        Ok(ClaimOutcome::Claimed(task))
    }

    fn owned_task(&self, id: &str, principal: &str) -> Result<Task> {
        let task = self.task(id)?;
        match &task.status {
            TaskStatus::Claimed { owner, .. } if owner == principal => Ok(task),
            _ => Err(CoordError::NotTaskOwner {
                task: id.to_owned(),
                principal: principal.to_owned(),
            }),
        }
    }

    /// Gives a claimed task back to the queue.
    pub fn release_task(&self, id: &str, principal: &str) -> Result<Task> {
        let _guard = self.locks.lock(&[TASKS_LOCK.to_owned()]);
        let mut task = self.owned_task(id, principal)?;
        task.status = TaskStatus::Open;
        self.save(TASK, &task.id, &task)?;
        Ok(task)
    }

    pub fn complete_task(&self, id: &str, principal: &str, summary: &str) -> Result<Task> {
        let _guard = self.locks.lock(&[TASKS_LOCK.to_owned()]);
        let mut task = self.owned_task(id, principal)?;
        task.status = TaskStatus::Done {
            owner: principal.to_owned(),
            summary: summary.to_owned(),
        };
        self.save(TASK, &task.id, &task)?;
        Ok(task)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use trellis_store::SqliteMetaStore;

    use super::*;
    use crate::env::TestEnv;

    pub(crate) fn coord() -> (Coord, Arc<TestEnv>) {
        let env = Arc::new(TestEnv::new());
        let coord = Coord::new(
            "r",
            Arc::new(SqliteMetaStore::open_in_memory().unwrap()),
            env.clone(),
        );
        (coord, env)
    }

    #[test]
    fn claim_next_hands_out_each_open_task_once_in_order() {
        let (c, _) = coord();
        let t1 = c.create_task("planner", "a", "", vec![]).unwrap();
        let t2 = c.create_task("planner", "b", "", vec![]).unwrap();
        let next = |who: &str| match c.claim_next(who).unwrap() {
            ClaimOutcome::Claimed(t) => Some(t.id),
            ClaimOutcome::NoneOpen => None,
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(next("x"), Some(t1.id.clone()));
        assert_eq!(next("y"), Some(t2.id.clone()));
        assert_eq!(next("z"), None);
        assert!(matches!(
            c.claim_next("x").unwrap(),
            ClaimOutcome::AtLimit { holding } if holding == vec![t1.id.clone()]
        ));
        c.complete_task(&t1.id, "x", "done").unwrap();
        assert_eq!(next("x"), None);
    }

    #[test]
    fn the_first_claim_wins_and_an_agent_holds_one_claim() {
        let (c, _) = coord();
        let t1 = c
            .create_task("planner", "ch1", "write chapter 1", vec![])
            .unwrap();
        let t2 = c
            .create_task("planner", "ch2", "write chapter 2", vec![])
            .unwrap();
        assert!(matches!(
            c.claim_task(&t1.id, "a").unwrap(),
            ClaimOutcome::Claimed(_)
        ));
        assert_eq!(
            c.claim_task(&t1.id, "b").unwrap(),
            ClaimOutcome::Taken { owner: "a".into() }
        );
        assert!(matches!(
            c.claim_task(&t1.id, "a").unwrap(),
            ClaimOutcome::Claimed(_)
        ));
        assert_eq!(
            c.claim_task(&t2.id, "a").unwrap(),
            ClaimOutcome::AtLimit {
                holding: vec![t1.id.clone()]
            }
        );
        assert!(matches!(
            c.claim_task(&t2.id, "b").unwrap(),
            ClaimOutcome::Claimed(_)
        ));
    }

    #[test]
    fn only_the_owner_completes_or_releases() {
        let (c, _) = coord();
        let t = c.create_task("planner", "x", "", vec![]).unwrap();
        c.claim_task(&t.id, "a").unwrap();
        assert!(matches!(
            c.complete_task(&t.id, "b", "nope"),
            Err(CoordError::NotTaskOwner { .. })
        ));
        assert_eq!(c.release_task(&t.id, "a").unwrap().status, TaskStatus::Open);
        c.claim_task(&t.id, "b").unwrap();
        let done = c.complete_task(&t.id, "b", "wrote it").unwrap();
        assert_eq!(
            done.status,
            TaskStatus::Done {
                owner: "b".into(),
                summary: "wrote it".into()
            }
        );
        assert_eq!(c.claim_task(&t.id, "c").unwrap(), ClaimOutcome::Finished);
        assert!(matches!(
            c.task("missing"),
            Err(CoordError::TaskNotFound(_))
        ));
        assert_eq!(c.tasks().unwrap().len(), 1);
    }

    #[test]
    fn the_claim_limit_is_configurable() {
        let (c, _) = coord();
        let c = c.with_max_claims(2);
        let t1 = c.create_task("p", "1", "", vec![]).unwrap();
        let t2 = c.create_task("p", "2", "", vec![]).unwrap();
        let t3 = c.create_task("p", "3", "", vec![]).unwrap();
        assert!(matches!(
            c.claim_task(&t1.id, "a").unwrap(),
            ClaimOutcome::Claimed(_)
        ));
        assert!(matches!(
            c.claim_task(&t2.id, "a").unwrap(),
            ClaimOutcome::Claimed(_)
        ));
        assert!(matches!(
            c.claim_task(&t3.id, "a").unwrap(),
            ClaimOutcome::AtLimit { .. }
        ));
    }

    #[test]
    fn concurrent_claims_have_one_winner() {
        let (c, _) = coord();
        let c = Arc::new(c);
        let t = c.create_task("p", "contested", "", vec![]).unwrap();
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let (c, id) = (Arc::clone(&c), t.id.clone());
                std::thread::spawn(move || c.claim_task(&id, &format!("agent-{i}")).unwrap())
            })
            .collect();
        let winners = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|o| matches!(o, ClaimOutcome::Claimed(_)))
            .count();
        assert_eq!(winners, 1);
    }
}

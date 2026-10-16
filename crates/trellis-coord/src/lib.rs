//! Coordination (spec §4.2, §4.5): tasks with atomic claims, leases on path scopes, and the
//! append-only context log. Pure coordination state over the metadata store; landing
//! (`trellis-land`) consults it.

mod context;
mod coord;
mod env;
mod error;
mod lease;
mod locks;
mod scope;
mod task;

pub use context::{Entry, EntryKind, EntryStatus, NewEntry};
pub use coord::Coord;
pub use env::{Env, SystemEnv, TestEnv};
pub use error::{CoordError, Result};
pub use lease::{Acquire, Lease, LeaseConflict, LeaseMode};
pub use locks::{Guard, KeyedLocks};
pub use scope::Scope;
pub use task::{ClaimOutcome, Task, TaskStatus};

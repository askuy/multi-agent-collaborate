//! Sessions and landing (spec §4.2, §4.4, §4.6): agents write into private sessions and the
//! engine lands them onto `main` with merges, a single CAS, receipts, and stored conflicts.

mod engine;
mod error;
mod policy;
mod records;

pub use engine::{Engine, Intent, IntentOutcome, MAIN, MAX_LAND_ATTEMPTS, RebaseOutcome};
pub use error::{LandError, Result};
pub use policy::{MergeStrategy, Policy, WriteMode};
pub use records::{
    ConflictPath, ConflictRecord, ConflictStatus, FileEntry, Notification, Outcome, Overlay,
    Receipt, Resolutions, Route, SessionRecord, SessionStatus,
};
pub use trellis_coord::{Env, SystemEnv, TestEnv};

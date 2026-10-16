//! Validation after landing (spec §4.7): instead of a merge queue, run checks on each
//! landed changeset (Scion's post-merge testing) and turn failures into FAIL log entries and
//! repair tasks that agents pick up.

mod runner;
mod spec;

pub use runner::{Finding, RunError, Runner};
pub use spec::{Severity, SpecError, ValidatorSpec, from_yaml};

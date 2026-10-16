//! Derived data (spec §4.8): indexes computed recursively from changesets and cached under
//! `(repo, changeset, name, version)`, so a new version can be re-derived side by side.

mod error;
mod framework;
mod paths;
mod pathset;
mod skiplist;
#[cfg(test)]
mod testutil;
mod tree;

pub use error::{DeriveError, Result};
pub use framework::{Derivable, DeriveCtx};
pub use paths::paths_changed_since;
pub use pathset::PathSet;
pub use skiplist::{SkipList, ancestor_at, first_parent_merge_base, is_first_parent_ancestor};
pub use tree::{Tree, TreeEntry, TreeIndex, flatten, lookup, read_file};

use std::collections::BTreeSet;

use trellis_core::{Hash, RepoPath};

use crate::error::{DeriveError, Result};
use crate::framework::DeriveCtx;
use crate::pathset::PathSet;
use crate::skiplist::is_first_parent_ancestor;

/// Every path changed by the changesets in `(from, to]` along `to`'s first-parent chain.
/// Landing uses this to find what changed on main since a session's base (spec §4.4).
pub fn paths_changed_since(
    ctx: &DeriveCtx<'_>,
    from: &Hash,
    to: &Hash,
) -> Result<BTreeSet<RepoPath>> {
    if !is_first_parent_ancestor(ctx, from, to)? {
        return Err(DeriveError::InvalidArgument(
            "`from` is not a first-parent ancestor of `to`",
        ));
    }
    let mut paths = BTreeSet::new();
    let mut current = *to;
    while current != *from {
        paths.extend(ctx.get::<PathSet>(&current)?.0);
        current = ctx
            .changeset(&current)?
            .first_parent()
            .ok_or(DeriveError::InvalidArgument("reached a root before `from`"))?;
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use crate::testutil::Fixture;

    use super::*;

    fn names(set: &BTreeSet<RepoPath>) -> Vec<&str> {
        set.iter().map(RepoPath::as_str).collect()
    }

    #[test]
    fn unions_paths_on_the_first_parent_range() {
        let fx = Fixture::new();
        let c1 = fx.commit(None, &[("a", Some("1")), ("b", Some("1"))]);
        let c2 = fx.commit(Some(c1), &[("b", Some("2")), ("c", Some("2"))]);
        let c3 = fx.commit(Some(c2), &[("d", Some("3"))]);
        let ctx = fx.ctx();
        assert_eq!(
            names(&paths_changed_since(&ctx, &c1, &c3).unwrap()),
            ["b", "c", "d"]
        );
        assert!(paths_changed_since(&ctx, &c3, &c3).unwrap().is_empty());
        assert!(matches!(
            paths_changed_since(&ctx, &c3, &c1),
            Err(DeriveError::InvalidArgument(_))
        ));
    }
}

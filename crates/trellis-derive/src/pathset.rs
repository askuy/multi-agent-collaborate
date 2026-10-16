use std::collections::BTreeSet;

use trellis_core::codec::{Reader, Writer};
use trellis_core::{Changeset, Hash, RepoPath};

use crate::error::Result;
use crate::framework::{Derivable, DeriveCtx};

const MAGIC: &[u8] = b"TRLS-PS\x01";

/// The paths one changeset changes (relative to its first parent).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PathSet(pub BTreeSet<RepoPath>);

impl Derivable for PathSet {
    const NAME: &'static str = "path_set";
    const VERSION: u32 = 1;

    fn derive(_ctx: &DeriveCtx<'_>, _id: &Hash, cs: &Changeset) -> Result<Self> {
        Ok(Self(cs.changes.keys().cloned().collect()))
    }

    fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(MAGIC);
        w.count(self.0.len());
        for path in &self.0 {
            w.str(path.as_str());
        }
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, MAGIC)?;
        let n = r.count()?;
        let mut paths = BTreeSet::new();
        for _ in 0..n {
            paths.insert(RepoPath::parse(&r.string()?)?);
        }
        r.finish()?;
        Ok(Self(paths))
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::Fixture;

    use super::*;

    #[test]
    fn path_set_lists_changed_paths_and_is_cached() {
        let fx = Fixture::new();
        let c1 = fx.commit(None, &[("a.txt", Some("1")), ("d/b.md", Some("2"))]);
        let c2 = fx.commit(Some(c1), &[("a.txt", None)]);
        let ctx = fx.ctx();

        let ps: PathSet = ctx.get(&c2).unwrap();
        assert_eq!(
            ps.0.iter().map(RepoPath::as_str).collect::<Vec<_>>(),
            ["a.txt"]
        );

        // Deriving c2 derived its ancestor c1 too, and stored both.
        let stored = fx
            .meta_get_derived(&c1, PathSet::NAME, PathSet::VERSION)
            .unwrap();
        let ps1 = PathSet::decode(&stored).unwrap();
        assert_eq!(
            ps1.0.iter().map(RepoPath::as_str).collect::<Vec<_>>(),
            ["a.txt", "d/b.md"]
        );
        assert_eq!(ctx.get::<PathSet>(&c1).unwrap(), ps1);
    }

    #[test]
    fn missing_changeset_is_an_error() {
        let fx = Fixture::new();
        let err = fx.ctx().get::<PathSet>(&Hash::of(b"nope")).unwrap_err();
        assert!(matches!(err, crate::DeriveError::MissingChangeset(_)));
    }

    #[test]
    fn deep_chains_derive_without_recursion() {
        let fx = Fixture::new();
        let ids = fx.chain(None, 3000);
        let ps: PathSet = fx.ctx().get(ids.last().unwrap()).unwrap();
        assert_eq!(ps.0.len(), 1);
    }
}

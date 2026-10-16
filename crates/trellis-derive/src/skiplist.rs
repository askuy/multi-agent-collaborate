use trellis_core::codec::{Reader, Writer};
use trellis_core::{Changeset, Hash};

use crate::error::{DeriveError, Result};
use crate::framework::{Derivable, DeriveCtx};

const MAGIC: &[u8] = b"TRLS-SL\x01";

/// First-parent depth and ancestor jumps: `skips[i]` is the first-parent ancestor `2^i`
/// steps back (spec §4.8, "skip lists for the commit graph" in Scion).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SkipList {
    pub generation: u64,
    pub skips: Vec<Hash>,
}

impl Derivable for SkipList {
    const NAME: &'static str = "skip_list";
    const VERSION: u32 = 1;

    fn derive(ctx: &DeriveCtx<'_>, _id: &Hash, cs: &Changeset) -> Result<Self> {
        let Some(parent) = cs.first_parent() else {
            return Ok(Self {
                generation: 0,
                skips: Vec::new(),
            });
        };
        let parent_list: SkipList = ctx.get(&parent)?;
        let mut skips = vec![parent];
        loop {
            let level = skips.len() - 1;
            let ancestor: SkipList = ctx.get(&skips[level])?;
            match ancestor.skips.get(level) {
                Some(next) => skips.push(*next),
                None => break,
            }
        }
        Ok(Self {
            generation: parent_list.generation + 1,
            skips,
        })
    }

    fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(MAGIC);
        w.u64(self.generation);
        w.count(self.skips.len());
        for h in &self.skips {
            w.hash(h);
        }
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, MAGIC)?;
        let generation = r.u64()?;
        let n = r.count()?;
        let mut skips = Vec::new();
        for _ in 0..n {
            skips.push(r.hash()?);
        }
        r.finish()?;
        Ok(Self { generation, skips })
    }
}

/// The first-parent ancestor of `id` at `generation`, in O(log distance) jumps.
pub fn ancestor_at(ctx: &DeriveCtx<'_>, id: &Hash, generation: u64) -> Result<Hash> {
    let mut current = *id;
    let mut list: SkipList = ctx.get(&current)?;
    if generation > list.generation {
        return Err(DeriveError::InvalidArgument(
            "generation is above the starting changeset",
        ));
    }
    while list.generation > generation {
        let distance = list.generation - generation;
        let level = (u64::BITS - 1 - distance.leading_zeros()) as usize;
        current = *list
            .skips
            .get(level)
            .ok_or(DeriveError::InvalidArgument("skip list ended early"))?;
        list = ctx.get(&current)?;
    }
    Ok(current)
}

/// Whether `ancestor` is `descendant` or one of its first-parent ancestors.
pub fn is_first_parent_ancestor(
    ctx: &DeriveCtx<'_>,
    ancestor: &Hash,
    descendant: &Hash,
) -> Result<bool> {
    let generation = ctx.get::<SkipList>(ancestor)?.generation;
    if generation > ctx.get::<SkipList>(descendant)?.generation {
        return Ok(false);
    }
    Ok(ancestor_at(ctx, descendant, generation)? == *ancestor)
}

/// The nearest common first-parent ancestor of `a` and `b`, or `None` if their histories
/// share no root.
pub fn first_parent_merge_base(ctx: &DeriveCtx<'_>, a: &Hash, b: &Hash) -> Result<Option<Hash>> {
    let level = ctx
        .get::<SkipList>(a)?
        .generation
        .min(ctx.get::<SkipList>(b)?.generation);
    let mut x = ancestor_at(ctx, a, level)?;
    let mut y = ancestor_at(ctx, b, level)?;
    loop {
        if x == y {
            return Ok(Some(x));
        }
        let sx: SkipList = ctx.get(&x)?;
        let sy: SkipList = ctx.get(&y)?;
        let Some(top) = sx.skips.len().min(sy.skips.len()).checked_sub(1) else {
            return Ok(None);
        };
        let jump = (0..=top)
            .rev()
            .find(|&i| sx.skips[i] != sy.skips[i])
            .unwrap_or(0);
        x = sx.skips[jump];
        y = sy.skips[jump];
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::Fixture;

    use super::*;

    #[test]
    fn generations_and_ancestors_match_a_naive_walk() {
        let fx = Fixture::new();
        let ids = fx.chain(None, 130);
        let ctx = fx.ctx();
        for (i, id) in ids.iter().enumerate() {
            let sl: SkipList = ctx.get(id).unwrap();
            assert_eq!(sl.generation, i as u64);
        }
        let tip = ids.last().unwrap();
        for (g, expected) in ids.iter().enumerate() {
            assert_eq!(
                ancestor_at(&ctx, tip, g as u64).unwrap(),
                *expected,
                "generation {g}"
            );
        }
        assert!(ancestor_at(&ctx, &ids[3], 4).is_err());
    }

    #[test]
    fn ancestry_checks() {
        let fx = Fixture::new();
        let ids = fx.chain(None, 40);
        let ctx = fx.ctx();
        assert!(is_first_parent_ancestor(&ctx, &ids[5], &ids[39]).unwrap());
        assert!(is_first_parent_ancestor(&ctx, &ids[39], &ids[39]).unwrap());
        assert!(!is_first_parent_ancestor(&ctx, &ids[39], &ids[5]).unwrap());
        let other = fx.chain(None, 3);
        assert!(!is_first_parent_ancestor(&ctx, &other[0], &ids[39]).unwrap());
    }

    #[test]
    fn merge_base_of_forked_chains() {
        let fx = Fixture::new();
        let trunk = fx.chain(None, 50);
        let fork_point = trunk[20];
        let left = fx.commit(Some(trunk[49]), &[("left.txt", Some("l"))]);
        let mut right_tip = fork_point;
        for i in 0..37 {
            let content = format!("r{i}");
            right_tip = fx.commit(Some(right_tip), &[("right.txt", Some(content.as_str()))]);
        }
        let ctx = fx.ctx();
        assert_eq!(
            first_parent_merge_base(&ctx, &left, &right_tip).unwrap(),
            Some(fork_point)
        );
        assert_eq!(
            first_parent_merge_base(&ctx, &right_tip, &left).unwrap(),
            Some(fork_point)
        );
        assert_eq!(
            first_parent_merge_base(&ctx, &left, &trunk[10]).unwrap(),
            Some(trunk[10])
        );
        assert_eq!(
            first_parent_merge_base(&ctx, &left, &left).unwrap(),
            Some(left)
        );
        let unrelated = fx.chain(None, 5);
        assert_eq!(
            first_parent_merge_base(&ctx, &left, &unrelated[4]).unwrap(),
            None
        );
    }
}

use std::collections::BTreeMap;

use trellis_core::codec::{Reader, Writer};
use trellis_core::{BlobRef, Changeset, CoreError, FileChange, FileKind, Hash, RepoPath};

use crate::error::{DeriveError, Result};
use crate::framework::{Derivable, DeriveCtx};

const TREE_MAGIC: &[u8] = b"TRLS-TR\x01";
const INDEX_MAGIC: &[u8] = b"TRLS-TI\x01";

/// One directory entry.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TreeEntry {
    File { blob: BlobRef, kind: FileKind },
    Dir(Hash),
}

/// One directory, stored as a content-addressed blob. Empty directories are never stored
/// except as the root of an empty repository.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Tree {
    pub entries: BTreeMap<String, TreeEntry>,
}

/// The root tree hash of the full file tree at a changeset (spec §4.1: the full tree is
/// derived data, never stored in the changeset).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TreeIndex {
    pub root: Hash,
}

impl Tree {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(TREE_MAGIC);
        w.count(self.entries.len());
        for (name, entry) in &self.entries {
            w.str(name);
            match entry {
                TreeEntry::File { blob, kind } => {
                    w.u8(1);
                    w.hash(&blob.hash);
                    w.u64(blob.size);
                    w.u8(kind.to_u8());
                }
                TreeEntry::Dir(hash) => {
                    w.u8(2);
                    w.hash(hash);
                }
            }
        }
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, TREE_MAGIC)?;
        let n = r.count()?;
        let mut entries = BTreeMap::new();
        let mut previous: Option<String> = None;
        for _ in 0..n {
            let name = r.string()?;
            if name.is_empty() || name.contains('/') || name == "." || name == ".." {
                return Err(CoreError::Decode("invalid tree entry name").into());
            }
            if previous.as_ref().is_some_and(|prev| prev >= &name) {
                return Err(CoreError::Decode("tree entries not strictly sorted").into());
            }
            let entry = match r.u8()? {
                1 => {
                    let hash = r.hash()?;
                    let size = r.u64()?;
                    let kind = FileKind::from_u8(r.u8()?)?;
                    TreeEntry::File {
                        blob: BlobRef { hash, size },
                        kind,
                    }
                }
                2 => TreeEntry::Dir(r.hash()?),
                _ => return Err(CoreError::Decode("unknown tree entry tag").into()),
            };
            previous = Some(name.clone());
            entries.insert(name, entry);
        }
        r.finish()?;
        Ok(Self { entries })
    }
}

pub(crate) fn load_tree(ctx: &DeriveCtx<'_>, hash: &Hash) -> Result<Tree> {
    let bytes = ctx
        .blobs
        .get(hash)?
        .ok_or(DeriveError::MissingObject(*hash))?;
    Tree::decode(&bytes)
}

fn store_tree(ctx: &DeriveCtx<'_>, tree: &Tree) -> Result<Hash> {
    Ok(ctx.blobs.put(&tree.encode())?.hash)
}

type Edits = Vec<(Vec<String>, FileChange)>;

/// Applies `changes` (paths split into segments, relative to this directory) to the
/// directory `dir`, writing only the directories on changed paths. Returns `None` when the
/// directory ends up empty. Cost is O(changes × depth), independent of repository size.
fn apply_dir(ctx: &DeriveCtx<'_>, dir: Option<Hash>, changes: Edits) -> Result<Option<Hash>> {
    let mut tree = match dir {
        Some(hash) => load_tree(ctx, &hash)?,
        None => Tree::default(),
    };
    let mut nested: BTreeMap<String, Edits> = BTreeMap::new();
    for (segments, change) in changes {
        let mut segments = segments.into_iter();
        let Some(head) = segments.next() else {
            continue;
        };
        let rest: Vec<String> = segments.collect();
        if rest.is_empty() {
            match change {
                FileChange::Upsert { blob, kind } => {
                    tree.entries.insert(head, TreeEntry::File { blob, kind });
                }
                FileChange::Delete => {
                    tree.entries.remove(&head);
                }
            }
        } else {
            nested.entry(head).or_default().push((rest, change));
        }
    }
    for (name, sub_changes) in nested {
        let existing = match tree.entries.get(&name) {
            Some(TreeEntry::Dir(hash)) => Some(*hash),
            _ => None,
        };
        match apply_dir(ctx, existing, sub_changes)? {
            Some(hash) => {
                tree.entries.insert(name, TreeEntry::Dir(hash));
            }
            None => {
                if existing.is_some() {
                    tree.entries.remove(&name);
                }
            }
        }
    }
    if tree.entries.is_empty() {
        return Ok(None);
    }
    Ok(Some(store_tree(ctx, &tree)?))
}

impl Derivable for TreeIndex {
    const NAME: &'static str = "tree_index";
    const VERSION: u32 = 1;

    fn derive(ctx: &DeriveCtx<'_>, _id: &Hash, cs: &Changeset) -> Result<Self> {
        let base = match cs.first_parent() {
            Some(parent) => Some(ctx.get::<TreeIndex>(&parent)?.root),
            None => None,
        };
        let changes: Edits = cs
            .changes
            .iter()
            .map(|(path, change)| (path.segments().map(str::to_owned).collect(), change.clone()))
            .collect();
        let root = match apply_dir(ctx, base, changes)? {
            Some(hash) => hash,
            None => store_tree(ctx, &Tree::default())?,
        };
        Ok(Self { root })
    }

    fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(INDEX_MAGIC);
        w.hash(&self.root);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, INDEX_MAGIC)?;
        let root = r.hash()?;
        r.finish()?;
        Ok(Self { root })
    }
}

/// The entry at `path` under the tree `root`, or `None`.
pub fn lookup(ctx: &DeriveCtx<'_>, root: &Hash, path: &RepoPath) -> Result<Option<TreeEntry>> {
    let mut tree = load_tree(ctx, root)?;
    let mut segments = path.segments().peekable();
    while let Some(segment) = segments.next() {
        let Some(entry) = tree.entries.get(segment).cloned() else {
            return Ok(None);
        };
        if segments.peek().is_none() {
            return Ok(Some(entry));
        }
        match entry {
            TreeEntry::Dir(hash) => tree = load_tree(ctx, &hash)?,
            TreeEntry::File { .. } => return Ok(None),
        }
    }
    Ok(None)
}

/// Every file under the tree `root`.
pub fn flatten(
    ctx: &DeriveCtx<'_>,
    root: &Hash,
) -> Result<BTreeMap<RepoPath, (BlobRef, FileKind)>> {
    let mut out = BTreeMap::new();
    flatten_into(ctx, root, "", &mut out)?;
    Ok(out)
}

fn flatten_into(
    ctx: &DeriveCtx<'_>,
    tree: &Hash,
    prefix: &str,
    out: &mut BTreeMap<RepoPath, (BlobRef, FileKind)>,
) -> Result<()> {
    for (name, entry) in load_tree(ctx, tree)?.entries {
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        match entry {
            TreeEntry::File { blob, kind } => {
                out.insert(RepoPath::parse(&path)?, (blob, kind));
            }
            TreeEntry::Dir(hash) => flatten_into(ctx, &hash, &path, out)?,
        }
    }
    Ok(())
}

/// The content of `path` at changeset `id`, or `None` if it is not a file there.
pub fn read_file(ctx: &DeriveCtx<'_>, id: &Hash, path: &RepoPath) -> Result<Option<Vec<u8>>> {
    let root = ctx.get::<TreeIndex>(id)?.root;
    match lookup(ctx, &root, path)? {
        Some(TreeEntry::File { blob, .. }) => Ok(Some(
            ctx.blobs
                .get(&blob.hash)?
                .ok_or(DeriveError::MissingObject(blob.hash))?,
        )),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use crate::testutil::Fixture;

    use super::*;

    fn read(fx: &Fixture, id: &Hash, path: &str) -> Option<String> {
        read_file(&fx.ctx(), id, &RepoPath::parse(path).unwrap())
            .unwrap()
            .map(|bytes| String::from_utf8(bytes).unwrap())
    }

    #[test]
    fn tree_tracks_upserts_deletes_and_empty_directories() {
        let fx = Fixture::new();
        let c1 = fx.commit(
            None,
            &[
                ("a.txt", Some("A")),
                ("d/e/f.md", Some("F")),
                ("d/g.json", Some("{}")),
            ],
        );
        let c2 = fx.commit(Some(c1), &[("d/e/f.md", None), ("a.txt", Some("A2"))]);
        assert_eq!(read(&fx, &c1, "d/e/f.md").as_deref(), Some("F"));
        assert_eq!(read(&fx, &c2, "d/e/f.md"), None);
        assert_eq!(read(&fx, &c2, "a.txt").as_deref(), Some("A2"));
        assert_eq!(read(&fx, &c2, "d/g.json").as_deref(), Some("{}"));

        let ctx = fx.ctx();
        let root = ctx.get::<TreeIndex>(&c2).unwrap().root;
        // "d/e" became empty and is removed; "d" survives because of "d/g.json".
        assert_eq!(
            lookup(&ctx, &root, &RepoPath::parse("d/e").unwrap()).unwrap(),
            None
        );
        assert!(matches!(
            lookup(&ctx, &root, &RepoPath::parse("d").unwrap()).unwrap(),
            Some(TreeEntry::Dir(_))
        ));
        // A path through a file is absent, not an error.
        assert_eq!(
            lookup(&ctx, &root, &RepoPath::parse("a.txt/x").unwrap()).unwrap(),
            None
        );
    }

    #[test]
    fn deleting_everything_yields_the_empty_tree() {
        let fx = Fixture::new();
        let c1 = fx.commit(None, &[("x/y.txt", Some("1"))]);
        let c2 = fx.commit(Some(c1), &[("x/y.txt", None)]);
        let ctx = fx.ctx();
        let root = ctx.get::<TreeIndex>(&c2).unwrap().root;
        assert_eq!(root, Hash::of(&Tree::default().encode()));
        assert!(flatten(&ctx, &root).unwrap().is_empty());
    }

    #[test]
    fn tree_decode_rejects_unsorted_or_invalid_names() {
        let mut w = Writer::new(TREE_MAGIC);
        w.count(2);
        w.str("b");
        w.u8(2);
        w.hash(&Hash::of(b"x"));
        w.str("a");
        w.u8(2);
        w.hash(&Hash::of(b"y"));
        assert!(Tree::decode(&w.finish()).is_err());

        let mut w = Writer::new(TREE_MAGIC);
        w.count(1);
        w.str("a/b");
        w.u8(2);
        w.hash(&Hash::of(b"x"));
        assert!(Tree::decode(&w.finish()).is_err());
    }

    const UNIVERSE: [&str; 6] = [
        "a.txt",
        "b.md",
        "d/x.txt",
        "d/e/y.json",
        "d/e/z.txt",
        "f/g/h/i.txt",
    ];

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Spec §7 invariant 7: the tree index equals a naive replay, and the root depends
        /// only on the final contents, not on the history that produced them.
        #[test]
        fn tree_index_matches_naive_replay(
            steps in prop::collection::vec(
                prop::collection::vec((0usize..UNIVERSE.len(), prop::option::of(0u8..4)), 1..5),
                1..12,
            )
        ) {
            let fx = Fixture::new();
            let ctx = fx.ctx();
            let mut naive: BTreeMap<String, String> = BTreeMap::new();
            let mut parent = None;
            for step in &steps {
                let mut edits: BTreeMap<&str, Option<String>> = BTreeMap::new();
                for (i, content) in step {
                    edits.insert(UNIVERSE[*i], content.map(|c| format!("content {c}")));
                }
                let refs: Vec<(&str, Option<&str>)> =
                    edits.iter().map(|(p, c)| (*p, c.as_deref())).collect();
                let id = fx.commit(parent, &refs);
                for (path, content) in &edits {
                    match content {
                        Some(c) => { naive.insert((*path).to_owned(), c.clone()); }
                        None => { naive.remove(*path); }
                    }
                }

                let root = ctx.get::<TreeIndex>(&id).unwrap().root;
                let actual: BTreeMap<String, String> = flatten(&ctx, &root)
                    .unwrap()
                    .into_iter()
                    .map(|(p, (blob, _))| {
                        let bytes = ctx.blobs.get(&blob.hash).unwrap().unwrap();
                        (p.as_str().to_owned(), String::from_utf8(bytes).unwrap())
                    })
                    .collect();
                prop_assert_eq!(&actual, &naive);

                let fresh_edits: Vec<(&str, Option<&str>)> =
                    naive.iter().map(|(p, c)| (p.as_str(), Some(c.as_str()))).collect();
                let fresh = fx.commit(None, &fresh_edits);
                prop_assert_eq!(ctx.get::<TreeIndex>(&fresh).unwrap().root, root);
                parent = Some(id);
            }
        }
    }
}

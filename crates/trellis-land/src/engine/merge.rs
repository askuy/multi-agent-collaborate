use std::collections::BTreeSet;

use trellis_core::{BlobRef, FileChange, FileKind, Hash, RepoPath};
use trellis_derive::{DeriveCtx, TreeEntry, TreeIndex, is_first_parent_ancestor, lookup};
use trellis_merge::{MergeResult, merge3, merge3_union};

use super::Engine;
use crate::error::{LandError, Result};
use crate::policy::{MergeStrategy, WriteMode};
use crate::records::{ConflictPath, Overlay, Resolutions};

/// What changed on `main` between a session's base and a target.
pub(crate) struct Scan {
    pub touched: BTreeSet<RepoPath>,
    /// A changeset in the range that this session already landed (a retry after a crash).
    pub already_landed: Option<Hash>,
}

pub(crate) enum MergeStep {
    Merged { overlay: Overlay, composed: bool },
    Conflicts(Vec<ConflictPath>),
    Strict(RepoPath),
}

fn file_at(
    ctx: &DeriveCtx<'_>,
    root: &Hash,
    path: &RepoPath,
) -> Result<Option<(BlobRef, FileKind)>> {
    Ok(match lookup(ctx, root, path)? {
        Some(TreeEntry::File { blob, kind }) => Some((blob, kind)),
        _ => None,
    })
}

/// Whether applying `change` leaves the path exactly as `current`.
fn is_noop(change: &FileChange, current: Option<(BlobRef, FileKind)>) -> bool {
    match (change, current) {
        (FileChange::Upsert { blob, .. }, Some((existing, _))) => blob.hash == existing.hash,
        (FileChange::Delete, None) => true,
        _ => false,
    }
}

/// Rejects a file and a directory sharing a path after `changes` are applied on `root`.
pub(crate) fn collision(
    ctx: &DeriveCtx<'_>,
    root: &Hash,
    changes: &Overlay,
) -> Result<Option<String>> {
    for (path, change) in changes {
        if !matches!(change, FileChange::Upsert { .. }) {
            continue;
        }
        let segments: Vec<&str> = path.segments().collect();
        for depth in 1..segments.len() {
            let prefix = RepoPath::parse(&segments[..depth].join("/"))?;
            let stays_a_file = match changes.get(&prefix) {
                Some(FileChange::Upsert { .. }) => true,
                Some(FileChange::Delete) => false,
                None => matches!(lookup(ctx, root, &prefix)?, Some(TreeEntry::File { .. })),
            };
            if stays_a_file {
                return Ok(Some(format!("{path} would be inside the file {prefix}")));
            }
        }
        if let Some(TreeEntry::Dir(_)) = lookup(ctx, root, path)? {
            return Ok(Some(format!("{path} is a directory on main")));
        }
    }
    Ok(None)
}

impl Engine {
    /// Walks `main`'s first-parent history from `target` back to `base`.
    pub(crate) fn scan_since(
        &self,
        ctx: &DeriveCtx<'_>,
        base: &Hash,
        target: &Hash,
        session: &str,
    ) -> Result<Scan> {
        if !is_first_parent_ancestor(ctx, base, target)? {
            return Err(LandError::BaseNotOnMain);
        }
        let mut touched = BTreeSet::new();
        let mut already_landed = None;
        let mut current = *target;
        while current != *base {
            let cs = ctx.changeset(&current)?;
            if already_landed.is_none() && cs.provenance.session.as_deref() == Some(session) {
                already_landed = Some(current);
            }
            touched.extend(cs.changes.keys().cloned());
            current = cs.first_parent().ok_or(LandError::BaseNotOnMain)?;
        }
        Ok(Scan {
            touched,
            already_landed,
        })
    }

    /// Rebases `overlay` from `base` onto `target`: untouched paths carry over, touched paths
    /// are three-way merged by file kind (or taken from `resolutions`), and writes that would
    /// leave a path exactly as on `target` are dropped.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn merge_overlay(
        &self,
        ctx: &DeriveCtx<'_>,
        base: &Hash,
        target: &Hash,
        overlay: &Overlay,
        touched: &BTreeSet<RepoPath>,
        resolutions: &Resolutions,
        enforce_strict: bool,
    ) -> Result<MergeStep> {
        let base_root = ctx.get::<TreeIndex>(base)?.root;
        let target_root = ctx.get::<TreeIndex>(target)?.root;
        let mut merged = Overlay::new();
        let mut conflicts = Vec::new();
        let mut composed = false;
        for (path, change) in overlay {
            let theirs = file_at(ctx, &target_root, path)?;
            if !touched.contains(path) {
                if !is_noop(change, theirs) {
                    merged.insert(path.clone(), change.clone());
                }
                continue;
            }
            if enforce_strict && self.policy.mode(path) == WriteMode::Strict {
                return Ok(MergeStep::Strict(path.clone()));
            }
            composed = true;
            if let Some(resolution) = resolutions.get(path) {
                let resolved = match resolution {
                    Some(bytes) => FileChange::Upsert {
                        blob: self.blobs.put(bytes)?,
                        kind: FileKind::infer(path, bytes),
                    },
                    None => FileChange::Delete,
                };
                if !is_noop(&resolved, theirs) {
                    merged.insert(path.clone(), resolved);
                }
                continue;
            }
            let base_file = file_at(ctx, &base_root, path)?;
            let (ours_blob, kind) = match change {
                FileChange::Upsert { blob, kind } => (Some(*blob), *kind),
                FileChange::Delete => (
                    None,
                    theirs.or(base_file).map_or(FileKind::Text, |(_, k)| k),
                ),
            };
            let load = |blob: Option<BlobRef>| blob.map(|b| self.read_blob(&b.hash)).transpose();
            let base_bytes = load(base_file.map(|(b, _)| b))?;
            let ours_bytes = load(ours_blob)?;
            let theirs_bytes = load(theirs.map(|(b, _)| b))?;
            let merge = match self.policy.merge(path) {
                MergeStrategy::Union => merge3_union,
                MergeStrategy::Auto => merge3,
            };
            match merge(
                kind,
                base_bytes.as_deref(),
                ours_bytes.as_deref(),
                theirs_bytes.as_deref(),
            ) {
                MergeResult::Clean(result) => {
                    let change = match result {
                        Some(bytes) => FileChange::Upsert {
                            blob: self.blobs.put(&bytes)?,
                            kind,
                        },
                        None => FileChange::Delete,
                    };
                    if !is_noop(&change, theirs) {
                        merged.insert(path.clone(), change);
                    }
                }
                MergeResult::Conflict(conflict) => conflicts.push(ConflictPath {
                    path: path.clone(),
                    kind,
                    base: base_file.map(|(b, _)| b),
                    ours: ours_blob,
                    theirs: theirs.map(|(b, _)| b),
                    reason: format!("{:?}", conflict.reason),
                    detail: conflict.detail,
                }),
            }
        }
        if conflicts.is_empty() {
            Ok(MergeStep::Merged {
                overlay: merged,
                composed,
            })
        } else {
            Ok(MergeStep::Conflicts(conflicts))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::{engine, p};

    #[test]
    fn collisions_between_files_and_directories_are_found() {
        let e = engine();
        // Put "dir/file.txt" and "top.txt" on main directly (landing arrives in Task 7).
        let blob = e.blobs.put(b"x").unwrap();
        let upsert = |path: &str| {
            (
                p(path),
                FileChange::Upsert {
                    blob,
                    kind: FileKind::Text,
                },
            )
        };
        let main = e.main().unwrap();
        let seed = trellis_core::Changeset {
            parents: vec![main],
            changes: [upsert("dir/file.txt"), upsert("top.txt")]
                .into_iter()
                .collect(),
            provenance: Default::default(),
            message: String::new(),
            created_at_ms: 0,
        };
        let id = e.meta.put_changeset(&e.repo, &seed).unwrap();
        assert!(
            e.meta
                .cas_ref(&e.repo, crate::engine::MAIN, Some(main), id)
                .unwrap()
        );

        let ctx = e.ctx();
        let root = ctx.get::<TreeIndex>(&e.main().unwrap()).unwrap().root;

        let inside_file: Overlay = [upsert("top.txt/child")].into_iter().collect();
        assert!(collision(&ctx, &root, &inside_file).unwrap().is_some());

        let over_dir: Overlay = [upsert("dir")].into_iter().collect();
        assert!(collision(&ctx, &root, &over_dir).unwrap().is_some());

        let file_then_child: Overlay = [upsert("new"), upsert("new/child")].into_iter().collect();
        assert!(collision(&ctx, &root, &file_then_child).unwrap().is_some());

        let replace_file_with_dir: Overlay =
            [(p("top.txt"), FileChange::Delete), upsert("top.txt/child")]
                .into_iter()
                .collect();
        assert_eq!(
            collision(&ctx, &root, &replace_file_with_dir).unwrap(),
            None
        );
    }
}

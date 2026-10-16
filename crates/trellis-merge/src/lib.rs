//! Three-way merge drivers chosen by [`FileKind`] (spec §4.2, the compose route).

mod structured;
mod text;

use trellis_core::FileKind;

/// The outcome of a three-way merge.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MergeResult {
    /// The merged content; `None` means the file ends up deleted.
    Clean(Option<Vec<u8>>),
    Conflict(Conflict),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConflictReason {
    ModifyDelete,
    TextOverlap,
    FieldOverlap,
    Binary,
    Unparseable,
}

/// Why a merge could not complete, with human- and agent-readable detail
/// (conflict markers for text, field paths for structured files).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Conflict {
    pub reason: ConflictReason,
    pub detail: String,
}

impl MergeResult {
    pub(crate) fn conflict(reason: ConflictReason, detail: impl Into<String>) -> Self {
        Self::Conflict(Conflict {
            reason,
            detail: detail.into(),
        })
    }
}

/// Merges `ours` and `theirs` against their common `base`; `None` means the file is absent.
pub fn merge3(
    kind: FileKind,
    base: Option<&[u8]>,
    ours: Option<&[u8]>,
    theirs: Option<&[u8]>,
) -> MergeResult {
    if ours == theirs || base == theirs {
        return MergeResult::Clean(ours.map(<[u8]>::to_vec));
    }
    if base == ours {
        return MergeResult::Clean(theirs.map(<[u8]>::to_vec));
    }
    let (Some(ours), Some(theirs)) = (ours, theirs) else {
        return MergeResult::conflict(
            ConflictReason::ModifyDelete,
            "one side deleted the file and the other changed it",
        );
    };
    match kind {
        FileKind::Text | FileKind::Markdown => text::merge(base.unwrap_or_default(), ours, theirs),
        FileKind::Json => structured::merge_json(base, ours, theirs),
        FileKind::Yaml => structured::merge_yaml(base, ours, theirs),
        FileKind::Binary => {
            MergeResult::conflict(ConflictReason::Binary, "both sides changed a binary file")
        }
    }
}

/// Like [`merge3`], but overlapping text edits keep both sides (ours first) instead of
/// conflicting. Everything else (clean merges, modify/delete, binary and structured files)
/// is exactly as in [`merge3`].
pub fn merge3_union(
    kind: FileKind,
    base: Option<&[u8]>,
    ours: Option<&[u8]>,
    theirs: Option<&[u8]>,
) -> MergeResult {
    match merge3(kind, base, ours, theirs) {
        MergeResult::Conflict(Conflict {
            reason: ConflictReason::TextOverlap,
            ..
        }) => match (ours, theirs) {
            (Some(ours), Some(theirs)) => text::union(base.unwrap_or_default(), ours, theirs),
            _ => merge3(kind, base, ours, theirs),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "one\ntwo\nthree\nfour\nfive\n";

    fn text(base: Option<&str>, ours: Option<&str>, theirs: Option<&str>) -> MergeResult {
        merge3(
            FileKind::Text,
            base.map(str::as_bytes),
            ours.map(str::as_bytes),
            theirs.map(str::as_bytes),
        )
    }

    fn clean(s: &str) -> MergeResult {
        MergeResult::Clean(Some(s.as_bytes().to_vec()))
    }

    #[test]
    fn trivial_cases_take_the_changed_side() {
        assert_eq!(text(Some(BASE), Some("x\n"), Some(BASE)), clean("x\n"));
        assert_eq!(text(Some(BASE), Some(BASE), Some("y\n")), clean("y\n"));
        assert_eq!(text(Some(BASE), Some("z\n"), Some("z\n")), clean("z\n"));
        assert_eq!(text(Some(BASE), None, None), MergeResult::Clean(None));
        assert_eq!(text(Some(BASE), None, Some(BASE)), MergeResult::Clean(None));
        assert_eq!(text(None, None, Some("new\n")), clean("new\n"));
    }

    #[test]
    fn modify_delete_conflicts() {
        let MergeResult::Conflict(c) = text(Some(BASE), None, Some("changed\n")) else {
            panic!("expected a conflict");
        };
        assert_eq!(c.reason, ConflictReason::ModifyDelete);
    }

    #[test]
    fn disjoint_text_edits_merge() {
        let ours = "ONE\ntwo\nthree\nfour\nfive\n";
        let theirs = "one\ntwo\nthree\nfour\nFIVE\n";
        assert_eq!(
            text(Some(BASE), Some(ours), Some(theirs)),
            clean("ONE\ntwo\nthree\nfour\nFIVE\n")
        );
    }

    #[test]
    fn overlapping_text_edits_conflict_with_markers() {
        let ours = "one\ntwo\nTHREE-A\nfour\nfive\n";
        let theirs = "one\ntwo\nTHREE-B\nfour\nfive\n";
        let MergeResult::Conflict(c) = text(Some(BASE), Some(ours), Some(theirs)) else {
            panic!("expected a conflict");
        };
        assert_eq!(c.reason, ConflictReason::TextOverlap);
        assert!(c.detail.contains("<<<<<<<"), "{}", c.detail);
    }

    #[test]
    fn binary_and_invalid_utf8_conflict() {
        let r = merge3(FileKind::Binary, Some(b"a"), Some(b"b"), Some(b"c"));
        assert!(matches!(
            r,
            MergeResult::Conflict(Conflict {
                reason: ConflictReason::Binary,
                ..
            })
        ));
        let r = merge3(FileKind::Text, Some(b"a"), Some(&[0xff]), Some(b"c"));
        assert!(matches!(
            r,
            MergeResult::Conflict(Conflict {
                reason: ConflictReason::Unparseable,
                ..
            })
        ));
    }

    fn union(base: Option<&str>, ours: Option<&str>, theirs: Option<&str>) -> MergeResult {
        merge3_union(
            FileKind::Markdown,
            base.map(str::as_bytes),
            ours.map(str::as_bytes),
            theirs.map(str::as_bytes),
        )
    }

    #[test]
    fn union_keeps_both_sides_of_an_overlap_ours_first() {
        let base = "# Changelog\n\n## Unreleased\n\n";
        let ours = "# Changelog\n\n## Unreleased\n\n- a (agent-1)\n";
        let theirs = "# Changelog\n\n## Unreleased\n\n- b (agent-2)\n";
        assert_eq!(
            union(Some(base), Some(ours), Some(theirs)),
            clean("# Changelog\n\n## Unreleased\n\n- a (agent-1)\n- b (agent-2)\n")
        );
    }

    #[test]
    fn union_matches_merge3_when_there_is_no_overlap() {
        let ours = "ONE\ntwo\nthree\nfour\nfive\n";
        let theirs = "one\ntwo\nthree\nfour\nFIVE\n";
        assert_eq!(
            union(Some(BASE), Some(ours), Some(theirs)),
            text(Some(BASE), Some(ours), Some(theirs))
        );
        assert_eq!(union(Some(BASE), Some("x\n"), Some(BASE)), clean("x\n"));
    }

    #[test]
    fn union_still_reports_modify_delete_and_leaves_other_kinds_alone() {
        assert!(matches!(
            union(Some(BASE), None, Some("changed\n")),
            MergeResult::Conflict(Conflict {
                reason: ConflictReason::ModifyDelete,
                ..
            })
        ));
        let r = merge3_union(FileKind::Binary, Some(b"a"), Some(b"b"), Some(b"c"));
        assert!(matches!(
            r,
            MergeResult::Conflict(Conflict {
                reason: ConflictReason::Binary,
                ..
            })
        ));
    }
}

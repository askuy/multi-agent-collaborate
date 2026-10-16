use crate::{ConflictReason, MergeResult};

/// Line-based three-way merge; overlapping edits produce conflict markers.
pub(crate) fn merge(base: &[u8], ours: &[u8], theirs: &[u8]) -> MergeResult {
    let (Ok(base), Ok(ours), Ok(theirs)) = (
        std::str::from_utf8(base),
        std::str::from_utf8(ours),
        std::str::from_utf8(theirs),
    ) else {
        return MergeResult::conflict(ConflictReason::Unparseable, "text is not valid UTF-8");
    };
    match diffy::merge(base, ours, theirs) {
        Ok(merged) => MergeResult::Clean(Some(merged.into_bytes())),
        Err(with_markers) => MergeResult::conflict(ConflictReason::TextOverlap, with_markers),
    }
}

/// Marker length for union merges: long enough never to collide with real content.
const UNION_MARKER: usize = 64;

/// Line-based three-way merge where overlapping hunks keep both sides, ours first
/// (Git's `merge=union`). For append-style files such as changelogs and logs.
pub(crate) fn union(base: &[u8], ours: &[u8], theirs: &[u8]) -> MergeResult {
    let (Ok(base), Ok(ours), Ok(theirs)) = (
        std::str::from_utf8(base),
        std::str::from_utf8(ours),
        std::str::from_utf8(theirs),
    ) else {
        return MergeResult::conflict(ConflictReason::Unparseable, "text is not valid UTF-8");
    };
    let mut options = diffy::MergeOptions::new();
    options
        .set_conflict_marker_length(UNION_MARKER)
        .set_conflict_style(diffy::ConflictStyle::Merge);
    let marked = match options.merge(base, ours, theirs) {
        Ok(merged) => return MergeResult::Clean(Some(merged.into_bytes())),
        Err(marked) => marked,
    };
    let open = format!("{} ours", "<".repeat(UNION_MARKER));
    let middle = "=".repeat(UNION_MARKER);
    let close = format!("{} theirs", ">".repeat(UNION_MARKER));
    let merged: String = marked
        .split_inclusive('\n')
        .filter(|line| {
            let bare = line.trim_end_matches('\n');
            bare != open && bare != middle && bare != close
        })
        .collect();
    MergeResult::Clean(Some(merged.into_bytes()))
}

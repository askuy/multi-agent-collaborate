# M4c: Union Merge for Append-Style Files — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the conflicts dogfooding run 1 hit (six, all concurrent insertions at the same place in `CHANGELOG.md`) by letting the policy mark append-style paths for a *union* merge, which keeps both sides' lines (as Git's `merge=union` attribute).

**Architecture:** `trellis-merge` gains `merge3_union`: it runs the normal `merge3`, and only when that ends in a text overlap it re-merges with diffy's two-sided conflict style and 64-character markers, then drops the markers, keeping our lines followed by theirs. Every other outcome (clean merges, modify/delete, binary, structured files) is unchanged. In `trellis-land`, a policy rule gets an optional `merge: union` next to its `mode`; the first rule matching a path decides both. `merge_overlay` calls `merge3_union` for union paths.

**Spec:** §4.2 (compose route), §4.11 (policy file). Finding 2 of `docs/dogfood/2026-10-15-run-1-textkit.md`.

---

### Task 1: `merge3_union`

**Files:** Modify `crates/trellis-merge/src/lib.rs`, `crates/trellis-merge/src/text.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `crates/trellis-merge/src/lib.rs`:

```rust
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
        assert_eq!(union(Some(BASE), Some(ours), Some(theirs)), text(Some(BASE), Some(ours), Some(theirs)));
        assert_eq!(union(Some(BASE), Some("x\n"), Some(BASE)), clean("x\n"));
    }

    #[test]
    fn union_still_reports_modify_delete_and_leaves_other_kinds_alone() {
        assert!(matches!(
            union(Some(BASE), None, Some("changed\n")),
            MergeResult::Conflict(Conflict { reason: ConflictReason::ModifyDelete, .. })
        ));
        let r = merge3_union(FileKind::Binary, Some(b"a"), Some(b"b"), Some(b"c"));
        assert!(matches!(r, MergeResult::Conflict(Conflict { reason: ConflictReason::Binary, .. })));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p trellis-merge union`
Expected: compile error, `cannot find function 'merge3_union'`.

- [ ] **Step 3: Implement**

Append to `crates/trellis-merge/src/text.rs`:

```rust
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
    options.set_conflict_marker_length(UNION_MARKER).set_conflict_style(diffy::ConflictStyle::Merge);
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
```

Add to `crates/trellis-merge/src/lib.rs`, after `merge3`:

```rust
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
        MergeResult::Conflict(Conflict { reason: ConflictReason::TextOverlap, .. }) => match (ours, theirs) {
            (Some(ours), Some(theirs)) => text::union(base.unwrap_or_default(), ours, theirs),
            _ => merge3(kind, base, ours, theirs),
        },
        other => other,
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p trellis-merge`
Expected: `test result: ok. 19 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-merge
git commit -m "feat(merge): add a union merge that keeps both sides of overlapping text edits"
```

---

### Task 2: `merge: union` in the policy, used by landing

**Files:** Modify `crates/trellis-land/src/policy.rs`, `crates/trellis-land/src/lib.rs`, `crates/trellis-land/src/engine/merge.rs`, `crates/trellis-land/src/engine/land.rs` (tests)

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `crates/trellis-land/src/policy.rs`:

```rust
    #[test]
    fn rules_can_ask_for_union_merges() {
        let policy = Policy::from_yaml(
            "rules:\n  - pattern: \"CHANGELOG.md\"\n    merge: union\n  - pattern: \"canon/**\"\n    mode: strict\n",
        )
        .unwrap();
        assert_eq!(policy.merge(&path("CHANGELOG.md")), MergeStrategy::Union);
        assert_eq!(policy.mode(&path("CHANGELOG.md")), WriteMode::Optimistic);
        assert_eq!(policy.merge(&path("canon/a.json")), MergeStrategy::Auto);
        assert_eq!(policy.merge(&path("src/a.rs")), MergeStrategy::Auto);
        assert!(Policy::from_yaml("rules: [{pattern: x, merge: sometimes}]").is_err());
    }
```

Append to the `tests` module in `crates/trellis-land/src/engine/land.rs`:

```rust
    #[test]
    fn union_paths_keep_both_sides_of_concurrent_appends() {
        let e = engine_with(Policy::from_yaml("rules:\n  - pattern: CHANGELOG.md\n    merge: union\n").unwrap());
        let seed = e.open_session("seed", None).unwrap();
        e.write(&seed.id, &p("CHANGELOG.md"), Some(b"## Unreleased\n\n")).unwrap();
        landed(&e.land(&seed.id, "seed", "k").unwrap().outcome);
        let a = e.open_session("a", None).unwrap();
        let b = e.open_session("b", None).unwrap();
        e.write(&a.id, &p("CHANGELOG.md"), Some(b"## Unreleased\n\n- a\n")).unwrap();
        e.write(&b.id, &p("CHANGELOG.md"), Some(b"## Unreleased\n\n- b\n")).unwrap();
        landed(&e.land(&a.id, "a", "k").unwrap().outcome);
        let (_, _, route) = landed(&e.land(&b.id, "b", "k").unwrap().outcome);
        assert_eq!(route, Route::Composed);
        assert_eq!(text(e.read_main(&p("CHANGELOG.md")).unwrap()).as_deref(), Some("## Unreleased\n\n- b\n- a\n"));
    }
```

(Session `b` lands second, so its lines are "ours" and come first.)

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p trellis-land union`
Expected: compile errors, `cannot find type 'MergeStrategy'` and `no method named 'merge' found for struct 'Policy'`.

- [ ] **Step 3: Implement**

In `crates/trellis-land/src/policy.rs`:

Add after `WriteMode`:

```rust
/// How landing merges a path that changed on both sides.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeStrategy {
    /// By file kind: line merge for text, field merge for JSON/YAML.
    #[default]
    Auto,
    /// Overlapping text edits keep both sides (append-style files such as changelogs).
    Union,
}
```

Change the rule storage and constructors:

```rust
/// Ordered glob rules; the first matching rule decides a path's mode and merge strategy.
#[derive(Clone, Debug, Default)]
pub struct Policy {
    rules: Vec<Rule>,
}

#[derive(Clone, Debug)]
struct Rule {
    pattern: String,
    matcher: GlobMatcher,
    mode: WriteMode,
    merge: MergeStrategy,
}
```

Replace `Policy::new`, `rule`, `mode`, and `serialize_keys` with:

```rust
impl Policy {
    pub fn new(rules: &[(&str, WriteMode)]) -> Result<Self> {
        Self::with_rules(rules.iter().map(|(pattern, mode)| (*pattern, *mode, MergeStrategy::Auto)))
    }

    fn with_rules<'a>(rules: impl IntoIterator<Item = (&'a str, WriteMode, MergeStrategy)>) -> Result<Self> {
        let rules = rules
            .into_iter()
            .map(|(pattern, mode, merge)| {
                let matcher = Glob::new(pattern)
                    .map_err(|e| LandError::Policy { pattern: pattern.to_owned(), message: e.to_string() })?
                    .compile_matcher();
                Ok(Rule { pattern: pattern.to_owned(), matcher, mode, merge })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { rules })
    }

    fn rule(&self, path: &RepoPath) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.matcher.is_match(path.as_str()))
    }

    pub fn mode(&self, path: &RepoPath) -> WriteMode {
        self.rule(path).map_or(WriteMode::Optimistic, |rule| rule.mode)
    }

    pub fn merge(&self, path: &RepoPath) -> MergeStrategy {
        self.rule(path).map_or(MergeStrategy::Auto, |rule| rule.merge)
    }

    /// The patterns of `Serialize` rules that decide any of `paths`, sorted and unique: the
    /// lock keys a landing must hold.
    pub fn serialize_keys<'a>(&self, paths: impl IntoIterator<Item = &'a RepoPath>) -> Vec<String> {
        let mut keys: Vec<String> = paths
            .into_iter()
            .filter_map(|path| match self.rule(path) {
                Some(rule) if rule.mode == WriteMode::Serialize => Some(rule.pattern.clone()),
                _ => None,
            })
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }
}
```

In the YAML structs, make `mode` optional and add `merge`:

```rust
#[derive(Deserialize)]
struct RuleFile {
    pattern: String,
    #[serde(default = "optimistic")]
    mode: WriteMode,
    #[serde(default)]
    merge: MergeStrategy,
}

fn optimistic() -> WriteMode {
    WriteMode::Optimistic
}
```

and in `from_yaml` build the policy with `Self::with_rules(file.rules.iter().map(|r| (r.pattern.as_str(), r.mode, r.merge)))` instead of `Self::new(&rules)`. Update the doc comment's YAML example to show `merge: union`.

In `crates/trellis-land/src/lib.rs`, change the policy re-export to `pub use policy::{MergeStrategy, Policy, WriteMode};`.

In `crates/trellis-land/src/engine/merge.rs`, import `trellis_merge::merge3_union` and `crate::policy::MergeStrategy`, and replace the `merge3(...)` call in `merge_overlay` with:

```rust
            let merged = match self.policy.merge(path) {
                MergeStrategy::Union => merge3_union(kind, base_bytes.as_deref(), ours_bytes.as_deref(), theirs_bytes.as_deref()),
                MergeStrategy::Auto => merge3(kind, base_bytes.as_deref(), ours_bytes.as_deref(), theirs_bytes.as_deref()),
            };
            match merged {
```

(keeping the existing match arms).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p trellis-land && cargo test -p trellis-sim`
Expected: land unit tests `test result: ok. 34 passed` and the concurrency test passes; sim passes.

- [ ] **Step 5: Use it in the dogfooding scenario and the README**

In `dogfood/code/policy.yaml`, add at the top:

```yaml
rules:
  - pattern: "CHANGELOG.md"
    merge: union
```

In `README.md`'s Validators section example, add a rule line `  - pattern: "CHANGELOG.md"` / `    merge: union             # concurrent appends keep both sides` under `rules:`.

- [ ] **Step 6: Verify and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: clippy exits 0; all suites `ok`.

```bash
git add crates/trellis-land dogfood/code/policy.yaml README.md docs/superpowers/plans/2026-10-15-m4c-union-merge.md
git commit -m "feat(land): let policy rules ask for union merges of append-style files"
```

use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};
use trellis_core::RepoPath;

use crate::error::{LandError, Result};

/// How landing treats a path that also changed on `main` since the session's base
/// (spec §4.2 admission routes).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WriteMode {
    /// Merge automatically by file kind (the compose route). The default.
    Optimistic,
    /// Merge automatically, but land one session at a time per matching pattern
    /// (lockfiles, migrations).
    Serialize,
    /// Never merge automatically: landing is rejected (fail-closed) if the path changed on
    /// `main` since the session's base. The agent must rebase and re-check its work.
    Strict,
}

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

impl Policy {
    pub fn new(rules: &[(&str, WriteMode)]) -> Result<Self> {
        Self::with_rules(
            rules
                .iter()
                .map(|(pattern, mode)| (*pattern, *mode, MergeStrategy::Auto)),
        )
    }

    fn with_rules<'a>(
        rules: impl IntoIterator<Item = (&'a str, WriteMode, MergeStrategy)>,
    ) -> Result<Self> {
        let rules = rules
            .into_iter()
            .map(|(pattern, mode, merge)| {
                let matcher = Glob::new(pattern)
                    .map_err(|e| LandError::Policy {
                        pattern: pattern.to_owned(),
                        message: e.to_string(),
                    })?
                    .compile_matcher();
                Ok(Rule {
                    pattern: pattern.to_owned(),
                    matcher,
                    mode,
                    merge,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { rules })
    }

    fn rule(&self, path: &RepoPath) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|rule| rule.matcher.is_match(path.as_str()))
    }

    pub fn mode(&self, path: &RepoPath) -> WriteMode {
        self.rule(path)
            .map_or(WriteMode::Optimistic, |rule| rule.mode)
    }

    pub fn merge(&self, path: &RepoPath) -> MergeStrategy {
        self.rule(path)
            .map_or(MergeStrategy::Auto, |rule| rule.merge)
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

#[derive(Deserialize, Default)]
struct PolicyFile {
    #[serde(default)]
    rules: Vec<RuleFile>,
}

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

impl Policy {
    /// Parses a `trellis.policy.yaml` (spec §4.11):
    ///
    /// ```yaml
    /// rules:
    ///   - pattern: "canon/**"
    ///     mode: strict
    ///   - pattern: "CHANGELOG.md"
    ///     merge: union
    /// ```
    pub fn from_yaml(text: &str) -> Result<Self> {
        let file: PolicyFile = if text.trim().is_empty() {
            PolicyFile::default()
        } else {
            serde_yaml::from_str(text).map_err(|e| LandError::Policy {
                pattern: "<policy file>".into(),
                message: e.to_string(),
            })?
        };
        Self::with_rules(
            file.rules
                .iter()
                .map(|r| (r.pattern.as_str(), r.mode, r.merge)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(p: &str) -> RepoPath {
        RepoPath::parse(p).unwrap()
    }

    #[test]
    fn first_matching_rule_wins_and_default_is_optimistic() {
        let policy = Policy::new(&[
            ("canon/secret.json", WriteMode::Strict),
            ("canon/**", WriteMode::Serialize),
            ("**/*.lock", WriteMode::Serialize),
        ])
        .unwrap();
        assert_eq!(policy.mode(&path("canon/secret.json")), WriteMode::Strict);
        assert_eq!(policy.mode(&path("canon/world.json")), WriteMode::Serialize);
        assert_eq!(policy.mode(&path("a/Cargo.lock")), WriteMode::Serialize);
        assert_eq!(policy.mode(&path("src/lib.rs")), WriteMode::Optimistic);
        assert_eq!(Policy::default().mode(&path("x")), WriteMode::Optimistic);
    }

    #[test]
    fn serialize_keys_are_sorted_and_unique() {
        let policy = Policy::new(&[
            ("canon/**", WriteMode::Serialize),
            ("*.lock", WriteMode::Serialize),
        ])
        .unwrap();
        let paths = [
            path("canon/a.json"),
            path("x.lock"),
            path("canon/b.json"),
            path("src/a.rs"),
        ];
        assert_eq!(policy.serialize_keys(paths.iter()), ["*.lock", "canon/**"]);
    }

    #[test]
    fn invalid_patterns_are_reported() {
        let err = Policy::new(&[("a[", WriteMode::Strict)]).unwrap_err();
        assert!(matches!(err, LandError::Policy { pattern, .. } if pattern == "a["));
    }

    #[test]
    fn loads_rules_from_a_yaml_policy_file() {
        let policy = Policy::from_yaml(
            "rules:\n  - pattern: \"canon/**\"\n    mode: strict\n  - pattern: \"**/*.lock\"\n    mode: serialize\n",
        )
        .unwrap();
        assert_eq!(policy.mode(&path("canon/a.json")), WriteMode::Strict);
        assert_eq!(policy.mode(&path("x/Cargo.lock")), WriteMode::Serialize);
        assert_eq!(policy.mode(&path("src/a.rs")), WriteMode::Optimistic);
        assert!(Policy::from_yaml("rules: [{pattern: \"a\", mode: sometimes}]").is_err());
        assert_eq!(
            Policy::from_yaml("").unwrap().mode(&path("a")),
            WriteMode::Optimistic
        );
    }

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
}

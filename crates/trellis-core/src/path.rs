use std::fmt;

use crate::error::{CoreError, Result};

/// A normalized, repository-relative path: `/`-separated, with no empty, `.`, or `..`
/// segments, no leading `/`, no backslash, and no NUL byte.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct RepoPath(String);

impl RepoPath {
    pub fn parse(s: &str) -> Result<Self> {
        let invalid = |reason: &'static str| -> Result<Self> {
            Err(CoreError::InvalidPath {
                path: s.to_owned(),
                reason,
            })
        };
        if s.is_empty() {
            return invalid("empty");
        }
        if s.starts_with('/') {
            return invalid("absolute");
        }
        if s.contains('\\') {
            return invalid("backslash");
        }
        if s.contains('\0') {
            return invalid("nul byte");
        }
        for segment in s.split('/') {
            match segment {
                "" => return invalid("empty segment"),
                "." | ".." => return invalid("dot segment"),
                _ => {}
            }
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn segments(&self) -> std::str::Split<'_, char> {
        self.0.split('/')
    }

    pub fn file_name(&self) -> &str {
        match self.0.rfind('/') {
            Some(i) => &self.0[i + 1..],
            None => &self.0,
        }
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl serde::Serialize for RepoPath {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for RepoPath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_paths() {
        for p in [
            "a",
            "a/b.md",
            "dir/sub/file.json",
            "名字/文件.md",
            ".hidden/x",
        ] {
            assert_eq!(RepoPath::parse(p).unwrap().as_str(), p);
        }
    }

    #[test]
    fn rejects_malformed_paths() {
        for p in [
            "", "/a", "a//b", "a/", "./a", "a/../b", "..", "a\\b", "a\0b",
        ] {
            assert!(RepoPath::parse(p).is_err(), "{p:?} should be rejected");
        }
    }

    #[test]
    fn exposes_segments_and_file_name() {
        let p = RepoPath::parse("a/b/c.md").unwrap();
        assert_eq!(p.file_name(), "c.md");
        assert_eq!(p.segments().collect::<Vec<_>>(), ["a", "b", "c.md"]);
        assert_eq!(RepoPath::parse("top").unwrap().file_name(), "top");
        assert_eq!(p.to_string(), "a/b/c.md");
    }
}

use std::fmt;

use trellis_core::RepoPath;

use crate::error::{CoordError, Result};

/// What a lease or a watch covers: one path, everything under a directory (`dir/**`), or
/// the whole repository (`**`). Overlap is exact and segment-aware, unlike general globs.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Scope {
    All,
    Dir(RepoPath),
    Path(RepoPath),
}

fn is_under(path: &RepoPath, dir: &RepoPath) -> bool {
    path.as_str()
        .strip_prefix(dir.as_str())
        .is_some_and(|rest| rest.starts_with('/'))
}

impl Scope {
    pub fn parse(s: &str) -> Result<Self> {
        let invalid = || CoordError::InvalidScope(s.to_owned());
        if s == "**" {
            return Ok(Self::All);
        }
        if let Some(dir) = s.strip_suffix("/**") {
            return RepoPath::parse(dir).map(Self::Dir).map_err(|_| invalid());
        }
        if s.contains('*') {
            return Err(invalid());
        }
        RepoPath::parse(s).map(Self::Path).map_err(|_| invalid())
    }

    pub fn covers(&self, path: &RepoPath) -> bool {
        match self {
            Self::All => true,
            Self::Path(p) => p == path,
            Self::Dir(dir) => is_under(path, dir),
        }
    }

    pub fn overlaps(&self, other: &Scope) -> bool {
        match (self, other) {
            (Self::All, _) | (_, Self::All) => true,
            (Self::Path(a), Self::Path(b)) => a == b,
            (Self::Dir(dir), Self::Path(path)) | (Self::Path(path), Self::Dir(dir)) => {
                is_under(path, dir)
            }
            (Self::Dir(a), Self::Dir(b)) => a == b || is_under(a, b) || is_under(b, a),
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => f.write_str("**"),
            Self::Dir(dir) => write!(f, "{dir}/**"),
            Self::Path(path) => write!(f, "{path}"),
        }
    }
}

impl serde::Serialize for Scope {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Scope {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rp(p: &str) -> RepoPath {
        RepoPath::parse(p).unwrap()
    }

    fn s(x: &str) -> Scope {
        Scope::parse(x).unwrap()
    }

    #[test]
    fn parses_paths_directories_and_everything() {
        assert_eq!(s("**"), Scope::All);
        assert_eq!(s("a/b/**"), Scope::Dir(rp("a/b")));
        assert_eq!(s("a/b.md"), Scope::Path(rp("a/b.md")));
        for bad in ["", "*", "a/*", "a/**/b", "/a", "a/../b", "**/x"] {
            assert!(Scope::parse(bad).is_err(), "{bad:?} should be rejected");
        }
        assert_eq!(s("a/**").to_string(), "a/**");
        assert_eq!(s("**").to_string(), "**");
    }

    #[test]
    fn covers_and_overlaps_respect_segment_boundaries() {
        let dir = s("a/**");
        assert!(dir.covers(&rp("a/x")));
        assert!(dir.covers(&rp("a/x/y")));
        assert!(!dir.covers(&rp("a")));
        assert!(!dir.covers(&rp("ab/x")));
        assert!(Scope::All.covers(&rp("z")));
        assert!(s("a/x").covers(&rp("a/x")));
        assert!(!s("a/x").covers(&rp("a/x/y")));

        assert!(dir.overlaps(&s("a/x")));
        assert!(s("a/x").overlaps(&dir));
        assert!(!dir.overlaps(&s("ab/**")));
        assert!(dir.overlaps(&s("a/b/**")));
        assert!(s("a/b/**").overlaps(&dir));
        assert!(s("a/x").overlaps(&s("a/x")));
        assert!(!s("a/x").overlaps(&s("a/y")));
        assert!(Scope::All.overlaps(&s("q")));
    }

    #[test]
    fn serializes_as_its_string_form() {
        let json = serde_json::to_string(&s("a/**")).unwrap();
        assert_eq!(json, "\"a/**\"");
        assert_eq!(serde_json::from_str::<Scope>(&json).unwrap(), s("a/**"));
        assert!(serde_json::from_str::<Scope>("\"a/*\"").is_err());
    }
}

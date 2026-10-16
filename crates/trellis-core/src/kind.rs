use crate::error::{CoreError, Result};
use crate::path::RepoPath;

/// How a file is merged and validated.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Text,
    Markdown,
    Json,
    Yaml,
    Binary,
}

impl FileKind {
    pub fn to_u8(self) -> u8 {
        match self {
            Self::Text => 1,
            Self::Markdown => 2,
            Self::Json => 3,
            Self::Yaml => 4,
            Self::Binary => 5,
        }
    }

    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Text),
            2 => Ok(Self::Markdown),
            3 => Ok(Self::Json),
            4 => Ok(Self::Yaml),
            5 => Ok(Self::Binary),
            _ => Err(CoreError::Decode("unknown file kind")),
        }
    }

    /// Binary if the content is not UTF-8 or has a NUL byte in its first 8 KiB;
    /// otherwise chosen by extension, defaulting to `Text`.
    pub fn infer(path: &RepoPath, content: &[u8]) -> Self {
        let head = &content[..content.len().min(8192)];
        if head.contains(&0) || std::str::from_utf8(content).is_err() {
            return Self::Binary;
        }
        let extension = path
            .file_name()
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase());
        match extension.as_deref() {
            Some("md" | "markdown") => Self::Markdown,
            Some("json") => Self::Json,
            Some("yaml" | "yml") => Self::Yaml,
            _ => Self::Text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(p: &str) -> RepoPath {
        RepoPath::parse(p).unwrap()
    }

    #[test]
    fn round_trips_through_u8() {
        for kind in [
            FileKind::Text,
            FileKind::Markdown,
            FileKind::Json,
            FileKind::Yaml,
            FileKind::Binary,
        ] {
            assert_eq!(FileKind::from_u8(kind.to_u8()).unwrap(), kind);
        }
        assert!(FileKind::from_u8(0).is_err());
        assert!(FileKind::from_u8(6).is_err());
    }

    #[test]
    fn infers_from_content_then_extension() {
        assert_eq!(
            FileKind::infer(&path("a/README.md"), b"# hi"),
            FileKind::Markdown
        );
        assert_eq!(
            FileKind::infer(&path("x.MARKDOWN"), b"hi"),
            FileKind::Markdown
        );
        assert_eq!(FileKind::infer(&path("c.json"), b"{}"), FileKind::Json);
        assert_eq!(FileKind::infer(&path("c.yml"), b"a: 1"), FileKind::Yaml);
        assert_eq!(FileKind::infer(&path("c.yaml"), b"a: 1"), FileKind::Yaml);
        assert_eq!(
            FileKind::infer(&path("main.rs"), b"fn main() {}"),
            FileKind::Text
        );
        assert_eq!(FileKind::infer(&path("Makefile"), b"all:"), FileKind::Text);
        assert_eq!(
            FileKind::infer(&path("img.md"), b"\x00\x01"),
            FileKind::Binary
        );
        assert_eq!(
            FileKind::infer(&path("bad.txt"), &[0xff, 0xfe]),
            FileKind::Binary
        );
    }
}

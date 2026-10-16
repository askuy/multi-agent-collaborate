/// Errors from core types and the canonical codec.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid path {path:?}: {reason}")]
    InvalidPath { path: String, reason: &'static str },
    #[error("invalid hash: {0}")]
    InvalidHash(String),
    #[error("decode error: {0}")]
    Decode(&'static str),
}

pub type Result<T> = std::result::Result<T, CoreError>;

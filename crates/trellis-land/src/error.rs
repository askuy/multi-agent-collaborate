use trellis_core::CoreError;
use trellis_derive::DeriveError;
use trellis_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum LandError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Derive(#[from] DeriveError),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Coord(#[from] trellis_coord::CoordError),
    #[error("record encoding: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("invalid policy pattern {pattern:?}: {message}")]
    Policy { pattern: String, message: String },
    #[error("repository is not initialized")]
    NotInitialized,
    #[error("session {0} not found")]
    SessionNotFound(String),
    #[error("session {0} is not open")]
    SessionNotOpen(String),
    #[error("conflict {0} not found")]
    ConflictNotFound(String),
    #[error("conflict {0} is no longer current")]
    ConflictNotCurrent(String),
    #[error("conflict {id} has unresolved paths: {paths:?}")]
    Unresolved { id: String, paths: Vec<String> },
    #[error("session base is not on main's history")]
    BaseNotOnMain,
    #[error("main kept moving; gave up after {0} attempts")]
    Contention(usize),
}

pub type Result<T> = std::result::Result<T, LandError>;

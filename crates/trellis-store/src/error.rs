use trellis_core::{CoreError, Hash};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("object {0} is corrupt")]
    Corrupt(Hash),
    #[error("store lock poisoned")]
    Poisoned,
}

pub type Result<T> = std::result::Result<T, StoreError>;

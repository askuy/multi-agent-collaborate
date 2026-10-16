use trellis_core::{CoreError, Hash};
use trellis_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum DeriveError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("changeset {0} not found")]
    MissingChangeset(Hash),
    #[error("object {0} not found")]
    MissingObject(Hash),
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),
}

pub type Result<T> = std::result::Result<T, DeriveError>;

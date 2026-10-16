use trellis_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum CoordError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("record encoding: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("invalid scope {0:?}: use a path, `dir/**`, or `**`")]
    InvalidScope(String),
    #[error("task {0} not found")]
    TaskNotFound(String),
    #[error("{principal} does not own task {task}")]
    NotTaskOwner { task: String, principal: String },
    #[error("lease {0} not found or expired")]
    LeaseNotFound(String),
    #[error("{principal} does not hold lease {lease}")]
    NotLeaseHolder { lease: String, principal: String },
    #[error("log entry {0} not found")]
    EntryNotFound(u64),
}

pub type Result<T> = std::result::Result<T, CoordError>;

//! Trellis storage: content-addressed blobs and the metadata store.

mod blob;
mod error;
mod meta;
mod sqlite;

pub use blob::{BlobStore, FsBlobStore, MemBlobStore};
pub use error::{Result, StoreError};
pub use meta::{MetaStore, RefState};
pub use sqlite::SqliteMetaStore;

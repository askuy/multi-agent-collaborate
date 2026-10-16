//! A blocking client for the Trellis server (spec §5.1) and directory checkouts (§4.9).

pub mod checkout;
mod client;
mod error;

pub use checkout::{Changes, Checkout};
pub use client::{ChangesetEntry, Client};
pub use error::{ClientError, Result};

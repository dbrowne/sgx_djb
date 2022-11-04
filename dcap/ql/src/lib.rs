// Copyright (c) 2022 MobileCoin Foundation

#![doc = include_str!("../README.md")]
#![deny(missing_docs, missing_debug_implementations)]

extern crate alloc;

mod quote3;
mod quote_enclave;

use mc_sgx_dcap_types::QlError;
pub use quote3::TryFromReport;
pub use quote_enclave::{LoadPolicyInitializer, PathInitializer, QeTargetInfo};

/// Errors interacting with quote library functions
#[derive(Clone, Debug, displaydoc::Display, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Error {
    /// Paths have already been initialized
    PathsInitialized,
    /// Error from SGX quoting library function: {0}
    QuoteLib(QlError),
    /// Failed to convert a path to a string.  Path {0}
    PathStringConversion(String),
    /// Path does not exist
    PathDoesNotExist(String),
    /// Path length is longer than the 259 character bytes allowed
    PathLengthTooLong(String),
    /// The quoting enclave load policy has already been initialized
    LoadPolicyInitialized,
}

impl From<QlError> for Error {
    fn from(src: QlError) -> Self {
        Self::QuoteLib(src)
    }
}

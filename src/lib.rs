#![doc = include_str!("../README.md")]
// only enables the `doc_cfg` feature when the `docsrs` configuration attribute is defined
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(
    docsrs,
    doc(html_logo_url = "https://github.com/bitcoindevkit/bdk/raw/master/static/bdk.png")
)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![no_std]
#![warn(missing_docs)]
#![allow(clippy::uninlined_format_args)]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[doc(hidden)]
#[macro_use]
pub extern crate alloc;
pub extern crate bdk_chain as chain;
#[cfg(feature = "file_store")]
pub extern crate bdk_file_store as file_store;
#[cfg(feature = "keys-bip39")]
pub extern crate bip39;
pub extern crate bitcoin;
pub extern crate miniscript;
pub extern crate serde;
pub extern crate serde_json;

pub mod descriptor;
pub mod keys;
pub mod psbt;
#[cfg(feature = "test-utils")]
pub mod test_utils;

#[cfg(feature = "test-utils")]
pub mod persist_test_utils;

mod types;
mod wallet;

pub(crate) use bdk_chain::collections;
#[cfg(feature = "rusqlite")]
pub use bdk_chain::rusqlite;
#[cfg(feature = "rusqlite")]
pub use bdk_chain::rusqlite_impl;
pub use descriptor::template;
pub use descriptor::HdKeyPaths;
pub use psbt::*;
pub use signer;
pub use signer::SignOptions;
pub use tx_builder::*;
pub use types::*;
pub use wallet::*;

/// Get the version of [`bdk_wallet`](crate) at runtime.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION", "unknown")
}

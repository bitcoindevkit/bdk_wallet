//! Module containing the multi-keychain [`Wallet`].

mod changeset;
pub mod keyring;
mod wallet;

pub use changeset::*;
pub use keyring::KeyRing;
pub use wallet::*;

/// Alias for [`DescriptorId`](bdk_chain::DescriptorId).
pub(crate) type Did = bdk_chain::DescriptorId;

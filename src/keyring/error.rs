//! Errors produced while building a [`KeyRing`](crate::keyring::KeyRing).

use crate::descriptor::DescriptorError;
use alloc::boxed::Box;
use core::fmt;
use miniscript::{Descriptor, DescriptorPublicKey};

/// Error returned when a descriptor cannot be added to a [`KeyRing`](crate::keyring::KeyRing).
#[derive(Debug)]
#[non_exhaustive]
pub enum KeyRingError<K> {
    /// The descriptor is invalid, does not match the keyring's network, or fails the checks
    /// applied to every wallet descriptor.
    Descriptor(DescriptorError),
    /// The keychain is already assigned to a different descriptor.
    ///
    /// A keychain identifies exactly one descriptor for the life of the wallet.
    KeychainAlreadyAssigned(K),
    /// The descriptor is already assigned to a different keychain.
    ///
    /// Two keychains sharing a descriptor cannot be told apart when attributing discovered
    /// outputs, so the indexer rejects it.
    DescriptorAlreadyAssigned(Box<Descriptor<DescriptorPublicKey>>),
}

impl<K> From<DescriptorError> for KeyRingError<K> {
    fn from(err: DescriptorError) -> Self {
        Self::Descriptor(err)
    }
}

impl<K: fmt::Debug> fmt::Display for KeyRingError<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Descriptor(e) => e.fmt(f),
            Self::KeychainAlreadyAssigned(keychain) => {
                write!(
                    f,
                    "keychain {keychain:?} is already assigned to a descriptor"
                )
            }
            Self::DescriptorAlreadyAssigned(descriptor) => {
                write!(
                    f,
                    "descriptor {descriptor} is already assigned to a keychain"
                )
            }
        }
    }
}

impl<K: fmt::Debug> core::error::Error for KeyRingError<K> {}

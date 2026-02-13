use alloc::boxed::Box;
use core::fmt;

use crate::descriptor::ExtendedDescriptor;
use miniscript::{Descriptor, DescriptorPublicKey};

/// Error corresponding to [`KeyRing`]
#[derive(Debug, PartialEq)]
pub enum KeyRingError<K> {
    /// The descriptors used to create/being added to [`KeyRing`] are invalid
    Descriptor(crate::descriptor::error::Error),
    /// The keychain exists in the [`KeyRing`] but mapped to a different descriptor
    KeychainAlreadyExists(K),
    /// The descriptor exists in the [`KeyRing`] but mapped to a different keychain
    DescAlreadyExists(Box<Descriptor<DescriptorPublicKey>>),
    /// No descriptor was provided
    DescMissing,
    /// Data is missing the network
    MissingNetwork,
    /// Network does not match.
    NetworkMismatch {
        /// The network that is loaded.
        loaded: bitcoin::Network,
        /// The expected network.
        expected: bitcoin::Network,
    },
    /// The keychain is missing,
    MissingKeychain(K),
    /// Descriptor does not match for the `keychain`.
    DescriptorMismatch {
        /// Keychain identifying the descriptor
        keychain: K,
        /// The loaded descriptor
        loaded: Box<ExtendedDescriptor>,
        /// The expected descriptor
        expected: Box<ExtendedDescriptor>,
    },
}

impl<K> fmt::Display for KeyRingError<K>
where
    K: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Descriptor(e) => e.fmt(f),
            Self::KeychainAlreadyExists(keychain) => {
                write!(f, "{keychain} is already assigned a different descriptor.")
            }
            Self::DescAlreadyExists(desc) => {
                write!(f, "{desc} is already assigned to a different keychain.")
            }
            Self::DescMissing => write!(f, "no descriptor was provided."),
            Self::MissingNetwork => write!(f, "no network was provided."),
            Self::NetworkMismatch { loaded, expected } => {
                write!(f, "Network mismatch: loaded {loaded}, expected {expected}")
            }
            Self::MissingKeychain(keychain) => {
                write!(f, "loaded data does not contain the keychain {keychain}")
            }
            Self::DescriptorMismatch {
                keychain,
                loaded,
                expected,
            } => {
                write!(
                    f,
                    "Descriptor mismatch for {} keychain: loaded {}, expected {}",
                    keychain, loaded, expected,
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl<K> std::error::Error for KeyRingError<K> where K: fmt::Display + core::fmt::Debug {}

impl<K> From<crate::descriptor::error::Error> for KeyRingError<K> {
    fn from(err: crate::descriptor::error::Error) -> Self {
        KeyRingError::Descriptor(err)
    }
}

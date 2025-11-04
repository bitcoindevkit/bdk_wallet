use crate::descriptor::{DescriptorError, ExtendedDescriptor};
use core::fmt;

/// Error when loading a `KeyRing`.
#[derive(Debug, PartialEq)]
pub enum LoadError<K> {
    /// There was a problem with the passed-in descriptor(s).
    Descriptor(crate::descriptor::DescriptorError),
    /// Data is missing the network.
    MissingNetwork,
    /// Data is not as expected.
    Mismatch(LoadMismatch<K>),
    /// The default keychain is missing.
    MissingDefaultKeychain,
    /// The keychain is missing,
    MissingKeychain(K),
}

/// A mismatch while loading the [`KeyRing`] from a [`ChangeSet`]
///
/// [`KeyRing`]: crate::keyring::KeyRing
/// [`ChangeSet`]: crate::keyring::ChangeSet
#[derive(Debug, PartialEq)]
pub enum LoadMismatch<K> {
    /// Network does not match.
    Network {
        /// The network that is loaded.
        loaded: bitcoin::Network,
        /// The expected network.
        expected: bitcoin::Network,
    },
    /// Descriptor does not match for the `keychain`.
    Descriptor {
        /// Keychain identifying the descriptor
        keychain: K,
        /// The loaded descriptor
        loaded: ExtendedDescriptor,
        /// The expected descriptor
        expected: ExtendedDescriptor,
    },
    /// The default keychain is not as expected
    DefaultKeychain {
        /// The loaded default keychain
        loaded: K,
        /// The expected default keychain
        expected: K,
    },
}

impl<K> fmt::Display for LoadError<K>
where
    K: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Descriptor(e) => e.fmt(f),
            Self::MissingNetwork => write!(f, "network is missing"),
            Self::MissingDefaultKeychain => write!(f, "default keychain is missing"),
            Self::Mismatch(e) => e.fmt(f),
            Self::MissingKeychain(keychain) => write!(f, "keychain {keychain} is missing"),
        }
    }
}

impl<K> fmt::Display for LoadMismatch<K>
where
    K: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network { loaded, expected } => {
                write!(f, "Network mismatch: loaded {loaded}, expected {expected}")
            }
            Self::Descriptor {
                keychain,
                loaded,
                expected,
            } => write!(
                f,
                "Descriptor mismatch for {} keychain: loaded {}, expected {}",
                keychain, loaded, expected
            ),
            Self::DefaultKeychain { loaded, expected } => write!(
                f,
                "Loaded: {loaded} as default keychain though expected: {expected}"
            ),
        }
    }
}

#[cfg(feature = "std")]
impl<K> std::error::Error for LoadError<K> where K: fmt::Debug + fmt::Display {}

impl<K> From<LoadMismatch<K>> for LoadError<K> {
    fn from(mismatch: LoadMismatch<K>) -> Self {
        Self::Mismatch(mismatch)
    }
}

impl<K> From<DescriptorError> for LoadError<K> {
    fn from(desc_error: DescriptorError) -> Self {
        Self::Descriptor(desc_error)
    }
}

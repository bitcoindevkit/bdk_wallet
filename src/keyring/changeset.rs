use crate::keyring::BTreeMap;

use bitcoin::Network;
use chain::Merge;
use miniscript::{Descriptor, DescriptorPublicKey};
use serde::{Deserialize, Serialize};

/// Represents changes to the `KeyRing`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet<K: Ord> {
    /// Network.
    pub network: Option<Network>,
    /// Added descriptors.
    pub descriptors: BTreeMap<K, Descriptor<DescriptorPublicKey>>,
    /// Default keychain
    pub default_keychain: Option<K>,
}

impl<K: Ord> Default for ChangeSet<K> {
    fn default() -> Self {
        Self {
            network: None,
            descriptors: Default::default(),
            default_keychain: None,
        }
    }
}

impl<K: Ord> Merge for ChangeSet<K> {
    fn merge(&mut self, other: Self) {
        // merge network
        if other.network.is_some() && self.network.is_none() {
            self.network = other.network;
        }
        // merge descriptors
        self.descriptors.extend(other.descriptors);

        // Note: if a new default keychain has been set, it will take precedence over the old one.
        if other.default_keychain.is_some() {
            self.default_keychain = other.default_keychain;
        }
    }

    fn is_empty(&self) -> bool {
        self.network.is_none() && self.descriptors.is_empty()
    }
}

//! [`KeyRing`].

use bdk_chain::{DescriptorExt, Merge};
use bitcoin::{
    secp256k1::{All, Secp256k1},
    Network,
};
use miniscript::{Descriptor, DescriptorPublicKey};
use serde::{Deserialize, Serialize};

use crate::collections::BTreeMap;
use crate::descriptor::{check_wallet_descriptor, IntoWalletDescriptor};
use crate::multi_keychain::Did;

/// KeyRing.
#[derive(Debug, Clone)]
pub struct KeyRing<K> {
    pub(crate) secp: Secp256k1<All>,
    pub(crate) network: Network,
    pub(crate) descriptors: BTreeMap<K, Descriptor<DescriptorPublicKey>>,
}

impl<K> KeyRing<K>
where
    K: Ord + Clone,
{
    /// Construct new [`KeyRing`] with the provided `network`.
    pub fn new(network: Network) -> Self {
        Self {
            secp: Secp256k1::new(),
            network,
            descriptors: BTreeMap::default(),
        }
    }

    /// Add descriptor, must not be [multipath](miniscript::Descriptor::is_multipath).
    pub fn add_descriptor(&mut self, keychain: K, descriptor: impl IntoWalletDescriptor) {
        let descriptor = descriptor
            .into_wallet_descriptor(&self.secp, self.network)
            .expect("err: invalid descriptor")
            .0;
        assert!(
            !descriptor.is_multipath(),
            "err: Use `add_multipath_descriptor` instead"
        );
        check_wallet_descriptor(&descriptor).expect("err: failed `check_wallet_descriptor`");

        self.descriptors.insert(keychain, descriptor);
    }

    /// Initial changeset.
    pub fn initial_changeset(&self) -> ChangeSet<K> {
        ChangeSet {
            network: Some(self.network),
            descriptors: self.descriptors.clone(),
        }
    }

    /// Construct from changeset.
    pub fn from_changeset(changeset: ChangeSet<K>) -> Option<Self> {
        Some(Self {
            secp: Secp256k1::new(),
            network: changeset.network?,
            descriptors: changeset.descriptors,
        })
    }
}

impl KeyRing<Did> {
    /// Add multipath descriptor.
    pub fn add_multipath_descriptor(&mut self, descriptor: impl IntoWalletDescriptor) {
        let descriptor = descriptor
            .into_wallet_descriptor(&self.secp, self.network)
            .expect("err: invalid descriptor")
            .0;
        assert!(
            descriptor.is_multipath(),
            "err: Use `add_descriptor` instead"
        );
        let descriptors = descriptor
            .into_single_descriptors()
            .expect("err: invalid descriptor");
        for descriptor in descriptors {
            let did = descriptor.descriptor_id();
            self.descriptors.insert(did, descriptor);
        }
    }
}

/// Represents changes to the `KeyRing`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet<K: Ord> {
    /// Network.
    pub network: Option<Network>,
    /// Added descriptors.
    pub descriptors: BTreeMap<K, Descriptor<DescriptorPublicKey>>,
}

impl<K: Ord> Default for ChangeSet<K> {
    fn default() -> Self {
        Self {
            network: None,
            descriptors: Default::default(),
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
    }

    fn is_empty(&self) -> bool {
        self.network.is_none() && self.descriptors.is_empty()
    }
}

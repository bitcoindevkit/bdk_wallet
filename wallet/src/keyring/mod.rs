// Bitcoin Dev Kit
// Written in 2020 by Alekos Filini <alekos.filini@gmail.com>
//
// Copyright (c) 2020-2021 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

//! The KeyRing is a utility type used to streamline the building of wallets that handle any number
//! of descriptors. It ensures descriptors are usable together, consistent with a given network,
//! and will work with a BDK `Wallet`.

/// Contains `Changeset` corresponding to `KeyRing`.
pub mod changeset;

use crate::descriptor::IntoWalletDescriptor;
use crate::keyring::changeset::ChangeSet;
use bitcoin::secp256k1::{All, Secp256k1};
use bitcoin::Network;
use miniscript::{Descriptor, DescriptorPublicKey};
use std::collections::BTreeMap;

/// KeyRing.
#[derive(Debug, Clone)]
pub struct KeyRing<K> {
    pub(crate) secp: Secp256k1<All>,
    pub(crate) network: Network,
    pub(crate) descriptors: BTreeMap<K, Descriptor<DescriptorPublicKey>>,
    pub(crate) default_keychain: K,
}

impl<K> KeyRing<K>
where
    K: Ord + Clone,
{
    /// Construct a new [`KeyRing`] with the provided `network` and a descriptor. This descriptor
    /// will automatically become your default keychain. You can change your default keychain
    /// upon adding new ones with [`KeyRing::add_descriptor`]. Note that you cannot use a
    /// multipath descriptor here.
    pub fn new(network: Network, keychain: K, descriptor: impl IntoWalletDescriptor) -> Self {
        let secp = Secp256k1::new();
        let descriptor = descriptor
            .into_wallet_descriptor(&secp, network)
            .expect("err: invalid descriptor")
            .0;
        assert!(
            !descriptor.is_multipath(),
            "err: Use `add_multipath_descriptor` instead"
        );
        Self {
            secp: Secp256k1::new(),
            network,
            descriptors: BTreeMap::from([(keychain.clone(), descriptor)]),
            default_keychain: keychain.clone(),
        }
    }

    /// Add a descriptor. Must not be [multipath](miniscript::Descriptor::is_multipath).
    pub fn add_descriptor(
        &mut self,
        keychain: K,
        descriptor: impl IntoWalletDescriptor,
        default: bool,
    ) {
        let descriptor = descriptor
            .into_wallet_descriptor(&self.secp, self.network)
            .expect("err: invalid descriptor")
            .0;
        assert!(
            !descriptor.is_multipath(),
            "err: Use `add_multipath_descriptor` instead"
        );

        if default {
            self.default_keychain = keychain.clone();
        }
        self.descriptors.insert(keychain, descriptor);
    }

    /// Returns the specified default keychain on the KeyRing.
    pub fn default_keychain(&self) -> K {
        self.default_keychain.clone()
    }

    /// Change the default keychain on this `KeyRing`.
    pub fn set_default_keychain(&mut self, keychain: K) {
        self.default_keychain = keychain;
    }

    /// Return all keychains on this `KeyRing`.
    pub fn list_keychains(&self) -> &BTreeMap<K, Descriptor<DescriptorPublicKey>> {
        &self.descriptors
    }

    /// Initial changeset.
    pub fn initial_changeset(&self) -> ChangeSet<K> {
        ChangeSet {
            network: Some(self.network),
            descriptors: self.descriptors.clone(),
            default_keychain: Some(self.default_keychain.clone()),
        }
    }

    /// Construct from changeset.
    pub fn from_changeset(changeset: ChangeSet<K>) -> Option<Self> {
        Some(Self {
            secp: Secp256k1::new(),
            network: changeset.network?,
            descriptors: changeset.descriptors,
            default_keychain: changeset.default_keychain?,
        })
    }
}

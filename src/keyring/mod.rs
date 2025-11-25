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
/// Contains error types corresponding to `KeyRing`.
pub mod error;

pub use changeset::ChangeSet;
pub use error::{LoadError, LoadMismatch};

use crate::chain::{DescriptorExt, Merge};
use crate::descriptor::check_wallet_descriptor;
use crate::descriptor::{DescriptorError, IntoWalletDescriptor};
use crate::wallet::DescriptorToExtract;
use alloc::collections::BTreeMap;
use bitcoin::secp256k1::{All, Secp256k1};
use bitcoin::Network;
use miniscript::{Descriptor, DescriptorPublicKey};

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
    /// upon adding new ones with [`KeyRing::add_descriptor`].
    ///
    /// This method returns [`DescriptorError`] if the provided descriptor is multipath , contains
    /// hardened derivation steps (in case of public descriptors) or fails miniscripts sanity
    /// checks.
    pub fn new(
        network: Network,
        keychain: K,
        descriptor: impl IntoWalletDescriptor,
    ) -> Result<Self, DescriptorError> {
        let secp = Secp256k1::new();
        let descriptor = descriptor.into_wallet_descriptor(&secp, network.into())?.0;
        check_wallet_descriptor(&descriptor)?;
        Ok(Self {
            secp: Secp256k1::new(),
            network,
            descriptors: BTreeMap::from([(keychain.clone(), descriptor)]),
            default_keychain: keychain.clone(),
        })
    }

    /// Get the [`Network`] corresponding to the [`KeyRing`]
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// Add a descriptor. Must not be [multipath](miniscript::Descriptor::is_multipath).
    /// This method returns [`DescriptorError`] if the provided descriptor is multipath, contains
    /// hardened derivation steps (in case of public descriptors) or fails miniscripts sanity
    /// checks. It also returns the error when exactly one of `keychain` or `descriptor` is
    /// already in the keyring.
    pub fn add_descriptor(
        &mut self,
        keychain: K,
        descriptor: impl IntoWalletDescriptor,
        default: bool,
    ) -> Result<ChangeSet<K>, DescriptorError> {
        let descriptor = descriptor
            .into_wallet_descriptor(&self.secp, self.network.into())?
            .0;
        check_wallet_descriptor(&descriptor)?;

        // if the descriptor or keychain already exist
        for (keychain_old, desc) in self.descriptors.iter() {
            if (desc == &descriptor) && (keychain_old != &keychain) {
                return Err(DescriptorError::DescAlreadyExists);
            }
            if (keychain_old == &keychain) && (desc != &descriptor) {
                return Err(DescriptorError::KeychainAlreadyExists);
            }
        }

        self.descriptors
            .insert(keychain.clone(), descriptor.clone());

        let mut changeset = ChangeSet::default();
        changeset.descriptors.insert(keychain.clone(), descriptor);

        if default {
            self.default_keychain = keychain.clone();
            changeset.default_keychain = Some(keychain);
        }

        Ok(changeset)
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

    /// Construct `KeyRing` from changeset.
    pub fn from_changeset(
        changeset: ChangeSet<K>,
        check_network: Option<bitcoin::Network>,
        check_descs: BTreeMap<K, Option<DescriptorToExtract>>, /* none means just check if
                                                                * keychain is there. */
        check_default: Option<K>,
    ) -> Result<Option<Self>, LoadError<K>> {
        if changeset.is_empty() {
            return Ok(None);
        }
        let secp = Secp256k1::new();

        // check network is present
        let loaded_network = changeset.network.ok_or(LoadError::MissingNetwork)?;

        // check network is as expected
        if let Some(expected_network) = check_network {
            if loaded_network != expected_network {
                return Err(LoadError::Mismatch(LoadMismatch::Network {
                    loaded: loaded_network,
                    expected: expected_network,
                }));
            }
        }

        // check the descriptors are valid
        for desc in changeset.descriptors.values() {
            check_wallet_descriptor(desc)?;
        }

        // check expected descriptors are present
        for (keychain, check_desc) in check_descs {
            match changeset.descriptors.get(&keychain) {
                None => Err(LoadError::MissingKeychain(keychain))?,
                Some(loaded_desc) => {
                    if let Some(make_desc) = check_desc {
                        let (exp_desc, _) = make_desc(&secp, loaded_network.into())?;
                        if exp_desc.descriptor_id() != loaded_desc.descriptor_id() {
                            Err(LoadMismatch::Descriptor {
                                keychain,
                                loaded: loaded_desc.clone(),
                                expected: exp_desc,
                            })?
                        }
                    }
                }
            }
        }

        // check if default keychain is present and loaded correctly.
        match &changeset.default_keychain {
            None => Err(LoadError::MissingDefaultKeychain)?,
            Some(loaded) => {
                if let Some(expected) = check_default {
                    if *loaded != expected {
                        Err(LoadMismatch::DefaultKeychain {
                            loaded: loaded.clone(),
                            expected,
                        })?
                    }
                }
            }
        }

        Ok(Some(Self {
            secp: Secp256k1::new(),
            network: loaded_network,
            descriptors: changeset.descriptors,
            default_keychain: changeset
                .default_keychain
                .expect("checked few lines back that default_keychain is not None "),
        }))
    }
}

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

use alloc::fmt;
pub use changeset::ChangeSet;
pub use error::KeyRingError;

use crate::chain::{DescriptorExt, Merge};
use crate::descriptor::check_wallet_descriptor;
use crate::descriptor::IntoWalletDescriptor;
use crate::wallet::DescriptorToExtract;
use alloc::boxed::Box;
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
}

impl<K> KeyRing<K>
where
    K: Ord + Clone + fmt::Debug,
{
    /// Construct a new [`KeyRing`] with the provided `network` and a descriptor. You can add
    /// keychains with [`KeyRing::add_descriptor`].
    ///
    /// This method returns [`DescriptorError`] if the provided descriptor is multipath, contains
    /// hardened derivation steps (in the case of public descriptors) or fails miniscripts sanity
    /// checks.
    pub fn new(
        network: Network,
        keychain: K,
        descriptor: impl IntoWalletDescriptor,
    ) -> Result<Self, KeyRingError<K>> {
        let secp = Secp256k1::new();
        let descriptor = descriptor.into_wallet_descriptor(&secp, network.into())?.0;
        check_wallet_descriptor(&descriptor)?;

        Ok(Self {
            secp: Secp256k1::new(),
            network,
            descriptors: BTreeMap::from([(keychain.clone(), descriptor)]),
        })
    }

    /// Construct a new [`KeyRing`] with the provided `network` and a <Keychain, Descriptor> map and
    /// the `default_keychain`.
    ///
    /// Specifying `default_keychain` as `None` will assign the first keychain according to the
    /// `Ord` implementation as the default.
    ///
    /// Uses [`KeyRing::new`] and [`KeyRing::add_descriptor`] underneath.
    pub fn new_with_descriptors<D: IntoWalletDescriptor>(
        network: Network,
        descriptors: BTreeMap<K, D>,
    ) -> Result<Self, KeyRingError<K>> {
        // ToDo: maybe we can use something more generic than a map?
        if descriptors.is_empty() {
            return Err(KeyRingError::DescMissing);
        };

        let mut desc_iter = descriptors.into_iter();
        let (keychain, desc) = desc_iter.next().expect("descriptors is non-empty");
        let mut keyring = KeyRing::new(network, keychain.clone(), desc)?;

        for (keychain, desc) in desc_iter {
            keyring.add_descriptor(keychain, desc)?;
        }

        Ok(keyring)
    }

    /// Get the [`Network`] corresponding to the [`KeyRing`]
    pub fn network(&self) -> Network {
        self.network
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
    ) -> Result<ChangeSet<K>, KeyRingError<K>> {
        let descriptor = descriptor
            .into_wallet_descriptor(&self.secp, self.network.into())?
            .0;
        check_wallet_descriptor(&descriptor)?;

        // if the descriptor or keychain already exist
        for (keychain_old, desc) in self.descriptors.iter() {
            if (desc == &descriptor) && (keychain_old != &keychain) {
                return Err(KeyRingError::DescAlreadyExists(Box::new(desc.clone())));
            }
            if (keychain_old == &keychain) && (desc != &descriptor) {
                return Err(KeyRingError::KeychainAlreadyExists(keychain));
            }
        }

        self.descriptors
            .insert(keychain.clone(), descriptor.clone());

        let mut changeset = ChangeSet::default();
        changeset.descriptors.insert(keychain.clone(), descriptor);

        Ok(changeset)
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
        }
    }

    /// Construct `KeyRing` from changeset.
    pub(crate) fn from_changeset(
        changeset: ChangeSet<K>,
        check_network: Option<bitcoin::Network>,
        check_descs: BTreeMap<K, Option<DescriptorToExtract>>, /* none means just check if
                                                                * keychain is there. */
    ) -> Result<Option<Self>, KeyRingError<K>> {
        if changeset.is_empty() {
            return Ok(None);
        }
        let secp = Secp256k1::new();

        // check network is present
        let loaded_network = changeset.network.ok_or(KeyRingError::MissingNetwork)?;

        // check network is as expected
        if let Some(expected_network) = check_network {
            if loaded_network != expected_network {
                return Err(KeyRingError::NetworkMismatch {
                    loaded: loaded_network,
                    expected: expected_network,
                });
            }
        }

        // check the descriptors are valid
        for desc in changeset.descriptors.values() {
            check_wallet_descriptor(desc).map_err(|err| KeyRingError::Descriptor(err))?;
        }

        // check expected descriptors are present
        for (keychain, check_desc) in check_descs {
            match changeset.descriptors.get(&keychain) {
                None => Err(KeyRingError::MissingKeychain(keychain))?,
                Some(loaded_desc) => {
                    if let Some(make_desc) = check_desc {
                        let (exp_desc, _) = make_desc(&secp, loaded_network.into())
                            .map_err(|err| KeyRingError::Descriptor(err))?;
                        if exp_desc.descriptor_id() != loaded_desc.descriptor_id() {
                            Err(KeyRingError::DescriptorMismatch {
                                keychain,
                                loaded: Box::new(loaded_desc.clone()),
                                expected: Box::new(exp_desc),
                            })?
                        }
                    }
                }
            }
        }

        Ok(Some(Self {
            secp: Secp256k1::new(),
            network: loaded_network,
            descriptors: changeset.descriptors,
        }))
    }
}

#[cfg(test)]
mod test {
    #[cfg(feature = "rusqlite")]
    #[test]
    fn test_persist() {
        use crate::keyring::{ChangeSet, KeyRing};
        use bdk_chain::rusqlite;
        use bdk_wallet::KeychainKind;
        use bitcoin::Network;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".bdk_example_keyring.sqlite");

        // create a keyring and persist it
        let desc1 = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/0/*)";
        let keychain1 = KeychainKind::External;
        let mut keyring = KeyRing::new(Network::Regtest, keychain1, desc1).unwrap();
        let changeset = keyring.initial_changeset();

        let mut conn = rusqlite::Connection::open(file_path).unwrap();
        let db_tx = conn.transaction().unwrap();

        ChangeSet::<KeychainKind>::init_sqlite_tables(&db_tx).unwrap();
        changeset.persist_to_sqlite(&db_tx).unwrap();
        db_tx.commit().unwrap();

        // add a descriptor to the keyring and persist again
        let desc2 = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/1/*)";
        let keychain2 = KeychainKind::Internal;
        let changeset2 = keyring.add_descriptor(keychain2, desc2).unwrap();

        let db_tx = conn.transaction().unwrap();
        changeset2.persist_to_sqlite(&db_tx).unwrap();
        db_tx.commit().unwrap();

        let db_tx = conn.transaction().unwrap();
        let keyring_read = KeyRing::from_changeset(
            ChangeSet::<KeychainKind>::from_sqlite(&db_tx).unwrap(),
            None,
            [].into(),
        )
        .unwrap()
        .unwrap();

        assert_eq!(keyring.list_keychains(), keyring_read.list_keychains());
        assert_eq!(keyring.network(), keyring_read.network());
    }
}

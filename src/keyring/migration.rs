//! This module provides helper functions and types to assist users in migrating data related to
//! descriptors when upgrading from version 2.0  of the `bdk_wallet` crate.
#[cfg(feature = "rusqlite")]
use super::{changeset::ChangeSet, KeyRing};
#[cfg(feature = "rusqlite")]
use bdk_chain::{
    rusqlite::{self, Connection, OptionalExtension},
    Impl,
};
#[cfg(feature = "rusqlite")]
use miniscript::{Descriptor, DescriptorPublicKey};

#[cfg(feature = "rusqlite")]
use crate::KeychainKind;

#[cfg(feature = "rusqlite")]
use std::string::{String, ToString};

#[cfg(feature = "rusqlite")]
/// The table name storing descriptors and network for 2.0 `Wallet`
pub const V2_TABLE_NAME: &str = "bdk_wallet";

#[cfg(feature = "rusqlite")]
impl<K: Ord> ChangeSet<K> {
    // Note `change_desc_keychain` is not an `Option` since the user can repeat the keychain
    // used as `desc_keychain`. Since `change_desc` if not present then `rusqlite` would return a
    // `None`, hence it would never make it to `keyring.descriptors`.
    /// Obtain a `KeyRing::ChangeSet` from a v2 `Wallet` sqlite db.
    pub fn from_v2(
        db: &mut Connection,
        desc_keychain: K,
        change_desc_keychain: K,
    ) -> rusqlite::Result<Self> {
        let mut changeset = ChangeSet::default();
        let db_tx = db.transaction()?;
        let mut stmt = db_tx.prepare(&format!(
            "SELECT descriptor, change_descriptor, network FROM {}",
            V2_TABLE_NAME,
        ))?;
        let row = stmt
            .query_row([], |row| {
                Ok((
                    row.get::<_, Option<Impl<Descriptor<DescriptorPublicKey>>>>("descriptor")?,
                    row.get::<_, Option<Impl<Descriptor<DescriptorPublicKey>>>>(
                        "change_descriptor",
                    )?,
                    row.get::<_, Option<Impl<bitcoin::Network>>>("network")?,
                ))
            })
            .optional()?;

        if let Some((desc, change_desc, network)) = row {
            changeset.network = network.map(Impl::into_inner);
            if let Some(desc) = desc.map(Impl::into_inner) {
                changeset.descriptors.insert(desc_keychain, desc);
            }
            if let Some(change_desc) = change_desc.map(Impl::into_inner) {
                changeset
                    .descriptors
                    .insert(change_desc_keychain, change_desc);
            }
        }
        Ok(changeset)
    }
}

#[cfg(feature = "rusqlite")]
impl KeyRing<KeychainKind> {
    /// Obtain a `KeyRing<KeychainKind>` from a sqlite `Connection` corresponding to a
    /// v2 `Wallet`.
    ///
    /// Note the `KeyRing<KeychainKind>` thus built has the `Network`, the external keychain and the
    /// internal keychain (if present) corresponding to the v2 `Wallet`.
    pub fn from_v2(db: &mut rusqlite::Connection) -> Result<Option<KeyRing<KeychainKind>>, String> {
        let changeset =
            ChangeSet::<KeychainKind>::from_v2(db, KeychainKind::External, KeychainKind::Internal)
                .map_err(|e| e.to_string())?;
        KeyRing::<KeychainKind>::from_changeset(changeset, None, [].into())
            .map_err(|e| e.to_string())
    }
}

#[cfg(feature = "rusqlite")]
impl ChangeSet<KeychainKind> {
    /// Obtain a `ChangeSet<KeychainKind>` from a sqlite `Connection` corresponding to a
    /// v2 `Wallet`.
    ///
    /// Note that `KeyRing<KeychainKind>` which can be built using `ChangeSet<KeychainKind>`
    /// (look at `KeyRing::from_changeset`) holds the `Network`, the external keychain and the
    /// internal keychain (if present) corresponding to the v2 `Wallet`.
    pub fn from_v2_to_keychainkind(db: &mut Connection) -> rusqlite::Result<Self> {
        ChangeSet::<KeychainKind>::from_v2(db, KeychainKind::External, KeychainKind::Internal)
    }
}

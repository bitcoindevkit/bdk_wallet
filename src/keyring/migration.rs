#![cfg(feature = "rusqlite")]
use super::changeset::ChangeSet;
use bdk_chain::{
    rusqlite::{self, Connection, OptionalExtension},
    Impl,
};
use miniscript::{Descriptor, DescriptorPublicKey};

/// The table name storing descriptors and network for 2.0 `Wallet`
pub const V2_TABLE_NAME: &str = "bdk_wallet";

impl<K: Ord> ChangeSet<K> {
    // Note `change_desc_keychain` is not an `Option` since the user can repeat the keychain
    // used as `desc_keychain`. Since `change_desc` if not present then `rusqlite` would return a
    // `None`, hence it would never make it to `keyring.descriptors`.
    /// Obtain a `KeyRing::ChangeSet` from a 2.0 `Wallet` sqlite db.
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

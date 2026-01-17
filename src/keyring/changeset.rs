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
        self.network.is_none() && self.descriptors.is_empty() && self.default_keychain.is_none()
    }
}

#[cfg(feature = "rusqlite")]
use chain::{
    rusqlite::{self, types::FromSql, OptionalExtension, ToSql},
    Impl,
};

#[cfg(feature = "rusqlite")]
impl<K> ChangeSet<K>
where
    K: Ord + Clone + ToSql + FromSql,
{
    /// Schema name for `KeyRing`
    pub const SCHEMA_NAME: &str = "bdk_keyring";
    /// Name of table storing the network
    pub const NETWORK_TABLE_NAME: &str = "bdk_network";
    /// Name of table storing the descriptors
    pub const DESCRIPTORS_TABLE_NAME: &str = "bdk_descriptor";

    /// Returns the v0 sqlite schema for [`ChangeSet`]
    pub fn schema_v0() -> alloc::string::String {
        format!(
            "CREATE TABLE {} ( \
                id INTEGER PRIMARY KEY NOT NULL, \
                network TEXT NOT NULL \
            ) STRICT ; \
            CREATE TABLE {} ( \
               keychain TEXT PRIMARY KEY NOT NULL, \
               descriptor TEXT UNIQUE NOT NULL, \
               is_default INTEGER NOT NULL CHECK ( is_default IN (0,1) ) \
            ) STRICT ;",
            Self::NETWORK_TABLE_NAME,
            Self::DESCRIPTORS_TABLE_NAME
        )
    }

    /// Initialize sqlite tables
    pub(crate) fn init_sqlite_tables(db_tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        bdk_chain::rusqlite_impl::migrate_schema(db_tx, Self::SCHEMA_NAME, &[&Self::schema_v0()])?;
        Ok(())
    }

    /// Construct the `KeyRing` from persistence
    ///
    /// Remember to call [`Self::init_sqlite_tables`] beforehand.
    pub(crate) fn from_sqlite(db_tx: &rusqlite::Transaction) -> rusqlite::Result<Self> {
        let mut changeset = Self::default();
        let mut network_stmt = db_tx.prepare(&format!(
            "SELECT network FROM {} WHERE id = 0",
            Self::NETWORK_TABLE_NAME,
        ))?;
        let row = network_stmt
            .query_row([], |row| row.get::<_, Impl<bitcoin::Network>>("network"))
            .optional()?;

        if let Some(Impl(network)) = row {
            changeset.network = Some(network);
        }

        let mut descriptor_stmt = db_tx.prepare(&format!(
            "SELECT keychain, descriptor, is_default FROM {}",
            Self::DESCRIPTORS_TABLE_NAME
        ))?;

        let rows = descriptor_stmt.query_map([], |row| {
            Ok((
                row.get::<_, K>("keychain")?,
                row.get::<_, Impl<Descriptor<DescriptorPublicKey>>>("descriptor")?,
                row.get::<_, u8>("is_default")?,
            ))
        })?;

        for row in rows {
            let (keychain, Impl(descriptor), is_default) = row?;
            changeset.descriptors.insert(keychain.clone(), descriptor);

            if is_default == 1 {
                changeset.default_keychain = Some(keychain);
            }
        }

        Ok(changeset)
    }

    /// Persist the `KeyRing`
    ///
    /// Remember to call [`Self::init_sqlite_tables`] beforehand.
    pub(crate) fn persist_to_sqlite(&self, db_tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        use rusqlite::named_params;
        let mut network_stmt = db_tx.prepare_cached(&format!(
            "INSERT OR IGNORE INTO {}(id, network) VALUES(:id, :network)",
            Self::NETWORK_TABLE_NAME
        ))?;

        if let Some(network) = self.network {
            network_stmt.execute(named_params! {
                ":id": 0,
                ":network": Impl(network)
            })?;
        }

        let mut descriptor_stmt = db_tx.prepare_cached(&format!(
            "INSERT OR IGNORE INTO {}(keychain, descriptor, is_default) VALUES(:keychain, :desc, :is_default)", Self::DESCRIPTORS_TABLE_NAME
        ))?;

        for (keychain, desc) in &self.descriptors {
            descriptor_stmt.execute(named_params! {
                ":keychain": keychain.clone(),
                ":desc": Impl(desc.clone()),
                ":is_default": 0,
            })?;
        }

        let mut remove_old_default_stmt = db_tx.prepare_cached(&format!(
            "UPDATE {} SET is_default = 0 WHERE is_default = 1",
            Self::DESCRIPTORS_TABLE_NAME,
        ))?;

        let mut add_default_stmt = db_tx.prepare_cached(&format!(
            "UPDATE {} SET is_default = 1 WHERE keychain = :keychain",
            Self::DESCRIPTORS_TABLE_NAME,
        ))?;

        if let Some(keychain) = &self.default_keychain {
            remove_old_default_stmt.execute(())?;
            add_default_stmt.execute(named_params! { ":keychain": keychain.clone(),})?;
        }

        Ok(())
    }
}

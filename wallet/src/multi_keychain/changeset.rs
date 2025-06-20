use bdk_chain::{
    indexed_tx_graph, keychain_txout, local_chain, tx_graph, ConfirmationBlockTime, DescriptorId,
    Merge,
};
use bitcoin::Network;
use miniscript::{Descriptor, DescriptorPublicKey};
use serde::{Deserialize, Serialize};

use crate::multi_keychain::keyring;

/// Change set.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ChangeSet<K: Ord> {
    /// Keyring changeset.
    pub keyring: keyring::ChangeSet<K>,
    /// Changes to the [`LocalChain`](local_chain::LocalChain).
    pub local_chain: local_chain::ChangeSet,
    /// Changes to [`TxGraph`](tx_graph::TxGraph).
    pub tx_graph: tx_graph::ChangeSet<ConfirmationBlockTime>,
    /// Changes to [`KeychainTxOutIndex`](keychain_txout::KeychainTxOutIndex).
    pub indexer: keychain_txout::ChangeSet,
}

impl<K: Ord> Default for ChangeSet<K> {
    fn default() -> Self {
        Self {
            keyring: Default::default(),
            local_chain: Default::default(),
            tx_graph: Default::default(),
            indexer: Default::default(),
        }
    }
}

impl<K: Ord> Merge for ChangeSet<K> {
    fn merge(&mut self, other: Self) {
        // merge keyring
        self.keyring.merge(other.keyring);

        // merge local chain, tx-graph, indexer
        Merge::merge(&mut self.local_chain, other.local_chain);
        Merge::merge(&mut self.tx_graph, other.tx_graph);
        Merge::merge(&mut self.indexer, other.indexer);
    }

    fn is_empty(&self) -> bool {
        self.keyring.is_empty()
            && self.local_chain.is_empty()
            && self.tx_graph.is_empty()
            && self.indexer.is_empty()
    }
}

#[cfg(feature = "rusqlite")]
use bdk_chain::rusqlite;

#[cfg(feature = "rusqlite")]
impl ChangeSet<DescriptorId> {
    /// Schema name for wallet.
    pub const WALLET_SCHEMA_NAME: &'static str = "bdk_wallet";
    /// Name of table to store wallet metainformation.
    pub const WALLET_TABLE_NAME: &'static str = "bdk_wallet";
    /// Name of table to store wallet descriptors.
    pub const DESCRIPTORS_TABLE_NAME: &'static str = "bdk_descriptor";

    /// Get v0 sqlite [ChangeSet] schema.
    pub fn schema_v0() -> alloc::string::String {
        format!(
            "CREATE TABLE {} ( \
                id INTEGER PRIMARY KEY NOT NULL, \
                network TEXT NOT NULL \
            ); \
            CREATE TABLE {} ( \
                descriptor_id TEXT PRIMARY KEY NOT NULL, \
                descriptor BLOB NOT NULL \
            );",
            Self::WALLET_TABLE_NAME,
            Self::DESCRIPTORS_TABLE_NAME,
        )
    }

    /// Initializes tables and returns the aggregate data if the database is non-empty
    /// otherwise returns `Ok(None)`.
    pub fn initialize(db_tx: &rusqlite::Transaction) -> rusqlite::Result<Option<Self>> {
        Self::init_sqlite_tables(db_tx)?;
        let changeset = Self::from_sqlite(db_tx)?;

        if changeset.is_empty() {
            Ok(None)
        } else {
            Ok(Some(changeset))
        }
    }

    /// Initialize SQLite tables.
    fn init_sqlite_tables(db_tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        crate::rusqlite_impl::migrate_schema(
            db_tx,
            Self::WALLET_SCHEMA_NAME,
            &[&Self::schema_v0()],
        )?;

        local_chain::ChangeSet::init_sqlite_tables(db_tx)?;
        tx_graph::ChangeSet::<ConfirmationBlockTime>::init_sqlite_tables(db_tx)?;
        keychain_txout::ChangeSet::init_sqlite_tables(db_tx)?;

        Ok(())
    }

    /// Construct self by reading all of the SQLite data. This should succeed
    /// even if attempting to read an empty database.
    fn from_sqlite(db_tx: &rusqlite::Transaction) -> rusqlite::Result<Self> {
        use bdk_chain::Impl;
        use rusqlite::OptionalExtension;
        let mut changeset = Self::default();

        let mut keyring = keyring::ChangeSet::default();

        // Read network
        let mut network_stmt = db_tx.prepare(&format!(
            "SELECT network FROM {} WHERE id = 0",
            Self::WALLET_TABLE_NAME,
        ))?;
        let row = network_stmt
            .query_row([], |row| row.get::<_, Impl<Network>>("network"))
            .optional()?;
        if let Some(Impl(network)) = row {
            keyring.network = Some(network);
        }

        // Read descriptors
        let mut descriptor_stmt = db_tx.prepare(&format!(
            "SELECT descriptor_id, descriptor FROM {}",
            Self::DESCRIPTORS_TABLE_NAME
        ))?;
        let rows = descriptor_stmt.query_map([], |row| {
            Ok((
                row.get::<_, Impl<DescriptorId>>("descriptor_id")?,
                row.get::<_, Impl<Descriptor<DescriptorPublicKey>>>("descriptor")?,
            ))
        })?;
        for row in rows {
            let (Impl(did), Impl(descriptor)) = row?;
            keyring.descriptors.insert(did, descriptor);
        }

        changeset.keyring = keyring;
        changeset.local_chain = local_chain::ChangeSet::from_sqlite(db_tx)?;
        changeset.tx_graph = tx_graph::ChangeSet::from_sqlite(db_tx)?;
        changeset.indexer = keychain_txout::ChangeSet::from_sqlite(db_tx)?;

        Ok(changeset)
    }

    /// Persist self to SQLite.
    pub fn persist_to_sqlite(&self, db_tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        use chain::rusqlite::named_params;
        use chain::Impl;

        let keyring = &self.keyring;

        // Write network
        let mut network_stmt = db_tx.prepare_cached(&format!(
            "REPLACE INTO {}(id, network) VALUES(:id, :network)",
            Self::WALLET_TABLE_NAME,
        ))?;
        if let Some(network) = keyring.network {
            network_stmt.execute(named_params! {
                ":id": 0,
                ":network": Impl(network),
            })?;
        }

        // Write descriptors
        let mut descriptor_stmt = db_tx.prepare_cached(&format!(
            "INSERT OR IGNORE INTO {}(descriptor_id, descriptor) VALUES(:descriptor_id, :descriptor)",
            Self::DESCRIPTORS_TABLE_NAME,
        ))?;
        for (&did, descriptor) in &keyring.descriptors {
            descriptor_stmt.execute(named_params! {
                ":descriptor_id": Impl(did),
                ":descriptor": Impl(descriptor.clone()),
            })?;
        }

        self.local_chain.persist_to_sqlite(db_tx)?;
        self.tx_graph.persist_to_sqlite(db_tx)?;
        self.indexer.persist_to_sqlite(db_tx)?;

        Ok(())
    }
}

impl<K: Ord> From<local_chain::ChangeSet> for ChangeSet<K> {
    fn from(local_chain: local_chain::ChangeSet) -> Self {
        Self {
            local_chain,
            ..Default::default()
        }
    }
}

impl<K: Ord> From<indexed_tx_graph::ChangeSet<ConfirmationBlockTime, keychain_txout::ChangeSet>>
    for ChangeSet<K>
{
    fn from(
        indexed_tx_graph: indexed_tx_graph::ChangeSet<
            ConfirmationBlockTime,
            keychain_txout::ChangeSet,
        >,
    ) -> Self {
        Self {
            tx_graph: indexed_tx_graph.tx_graph,
            indexer: indexed_tx_graph.indexer,
            ..Default::default()
        }
    }
}

impl<K: Ord> From<keychain_txout::ChangeSet> for ChangeSet<K> {
    fn from(indexer: keychain_txout::ChangeSet) -> Self {
        Self {
            indexer,
            ..Default::default()
        }
    }
}

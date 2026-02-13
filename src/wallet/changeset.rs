use alloc::collections::btree_map::BTreeMap;
use bdk_chain::{
    indexed_tx_graph, keychain_txout, local_chain, tx_graph, ConfirmationBlockTime, Merge,
};
use miniscript::{Descriptor, DescriptorPublicKey};
use serde::{Deserialize, Serialize};

use crate::locked_outpoints;

type IndexedTxGraphChangeSet =
    indexed_tx_graph::ChangeSet<ConfirmationBlockTime, keychain_txout::ChangeSet>;

use crate::keyring;

#[cfg(feature = "rusqlite")]
use chain::{
    rusqlite::{
        self,
        types::{FromSql, ToSql},
    },
    Impl,
};

/// A change set for [`Wallet`]
///
/// ## Definition
///
/// The change set is responsible for transmitting data between the persistent storage layer and the
/// core library components. Specifically, it serves two primary functions:
///
/// 1) Recording incremental changes to the in-memory representation that need to be persisted to
///    disk
/// 2) Applying aggregate changes from the persistence layer to the in-memory representation at
///    startup
///
/// ## Contract
///
/// The change set maintains and enforces the following properties:
///
/// * Change sets must implement [`Serialize`] and [`Deserialize`] to meet the definition from
///   above.
/// * Change sets must implement [`Default`] as a way of instantiating new empty objects.
/// * Change sets must implement [`Merge`] so that many instances can be aggregated into a single
///   instance.
/// * A change set is composed of a number of individual "sub-change sets" that adhere to the same
///   rules as above. This is for increased modularity and portability. For example the core modules
///   each have their own change set (`tx_graph`, `local_chain`, etc).
///
/// ## Members and required fields
///
/// The change set has certain required fields without which a [`Wallet`] cannot function.
/// These include the [`descriptor`] and the [`bitcoin::Network`] in use. These are required to be
/// non-empty *in the aggregate*, meaning the field must be present and non-null in the union of all
/// persisted changes, but may be empty in any one change set, where "empty" is defined by the
/// [`Merge`](Merge::is_empty) implementation of that change set. This requirement also applies to
/// the [`local_chain`] field in that the aggregate change set must include a genesis block.
///
/// For example, the [`descriptor`] and [`bitcoin::Network`] are present in the first change set
/// after wallet creation, but are usually omitted in subsequent updates, as they are not permitted
/// to change at any point thereafter.
///
/// Other fields of the change set are not required to be non-empty, that is they may be empty even
/// in the aggregate. However, in practice they should contain the data needed to recover a wallet
/// state between sessions. These include:
/// * [`tx_graph`](Self::tx_graph)
/// * [`indexer`](Self::indexer)
///
/// The [`change_descriptor`] is special in that its presence is optional, however the value of the
/// change descriptor should be defined at wallet creation time and respected for the life of the
/// wallet, meaning that if a change descriptor is originally defined, it must also be present in
/// the aggregate change set.
///
/// ## Staging
///
/// For greater efficiency the [`Wallet`] is able to *stage* the to-be-persisted changes. Many
/// operations result in staged changes which require persistence on the part of the user. These
/// include address revelation, applying an [`Update`], and introducing transactions and chain
/// data to the wallet. To get the staged changes see [`Wallet::staged`] and similar methods. Once
/// the changes are committed to the persistence layer the contents of the stage should be
/// discarded.
///
/// Users should persist early and often generally speaking, however in principle there is no
/// limit to the number or type of changes that can be staged prior to persisting or the order in
/// which they're staged. This is because change sets are designed to be [merged]. The change
/// that is ultimately persisted will encompass the combined effect of each change individually.
///
/// ## Extensibility
///
/// Existing fields may be extended in the future with additional sub-fields. New top-level fields
/// are likely to be added as new features and core components are implemented. Existing fields may
/// be removed in future versions of the library.
///
/// The authors reserve the right to make breaking changes to the [`ChangeSet`] structure in
/// a major version release. API changes affecting the types of data persisted will display
/// prominently in the release notes. Users are advised to look for such changes and update their
/// application accordingly.
///
/// The resulting interface is designed to give the user more control of what to persist and when
/// to persist it. Custom implementations should consider and account for the possibility of
/// partial or repeat writes, the atomicity of persistence operations, and the order of reads and
/// writes among the fields of the change set. BDK comes with support for [SQLite] that handles
/// the details for you and is recommended for many users. If implementing your own persistence,
/// please refer to the documentation for [`WalletPersister`] and [`PersistedWallet`] for more
/// information.
///
/// [`change_descriptor`]: Self::change_descriptor
/// [`descriptor`]: Self::descriptor
/// [`local_chain`]: Self::local_chain
/// [merged]: bdk_chain::Merge
/// [`network`]: Self::network
/// [`PersistedWallet`]: crate::PersistedWallet
/// [SQLite]: bdk_chain::rusqlite_impl
/// [`Update`]: crate::Update
/// [`WalletPersister`]: crate::WalletPersister
/// [`Wallet::staged`]: crate::Wallet::staged
/// [`Wallet`]: crate::Wallet
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ChangeSet<K: Ord> {
    /// Stores the `KeyRing` containing the network and descriptor data.
    pub keyring: keyring::changeset::ChangeSet<K>,
    /// Changes to the [`LocalChain`](local_chain::LocalChain).
    pub local_chain: local_chain::ChangeSet,
    /// Changes to [`TxGraph`](tx_graph::TxGraph).
    pub tx_graph: tx_graph::ChangeSet<ConfirmationBlockTime>,
    /// Changes to [`KeychainTxOutIndex`](keychain_txout::KeychainTxOutIndex).
    pub indexer: keychain_txout::ChangeSet,
    /// Changes to locked outpoints.
    pub locked_outpoints: locked_outpoints::ChangeSet,
}

impl<K> Default for ChangeSet<K>
where
    K: Ord,
{
    fn default() -> Self {
        Self {
            keyring: Default::default(),
            local_chain: Default::default(),
            tx_graph: Default::default(),
            indexer: Default::default(),
            locked_outpoints: Default::default(),
        }
    }
}

impl<K> Merge for ChangeSet<K>
where
    K: Ord,
{
    /// Merge another [`ChangeSet`] into itself.
    fn merge(&mut self, other: Self) {
        // merge locked outpoints
        self.locked_outpoints.merge(other.locked_outpoints);
        Merge::merge(&mut self.keyring, other.keyring);
        Merge::merge(&mut self.local_chain, other.local_chain);
        Merge::merge(&mut self.tx_graph, other.tx_graph);
        Merge::merge(&mut self.indexer, other.indexer);
    }

    fn is_empty(&self) -> bool {
        self.keyring.is_empty()
            && self.local_chain.is_empty()
            && self.tx_graph.is_empty()
            && self.indexer.is_empty()
            && self.locked_outpoints.is_empty()
    }
}

#[cfg(feature = "rusqlite")]
impl<K> ChangeSet<K>
where
    K: Ord + Clone + FromSql + ToSql,
{
    /// Schema name for wallet.
    pub const WALLET_SCHEMA_NAME: &'static str = "bdk_wallet";
    /// Name of table to store wallet locked outpoints.
    pub const WALLET_OUTPOINT_LOCK_TABLE_NAME: &'static str = "bdk_wallet_locked_outpoints";

    /// Get v1 sqlite [`ChangeSet`] schema. Schema v1 adds a table for locked outpoints.
    pub fn schema_v0() -> alloc::string::String {
        format!(
            "CREATE TABLE {} ( \
                txid TEXT NOT NULL, \
                vout INTEGER NOT NULL, \
                PRIMARY KEY(txid, vout) \
                ) STRICT;",
            Self::WALLET_OUTPOINT_LOCK_TABLE_NAME,
        )
    }

    /// Initialize sqlite tables for wallet tables.
    pub fn init_sqlite_tables(db_tx: &chain::rusqlite::Transaction) -> chain::rusqlite::Result<()> {
        crate::rusqlite_impl::migrate_schema(
            db_tx,
            Self::WALLET_SCHEMA_NAME,
            &[&Self::schema_v0()],
        )?;
        keyring::changeset::ChangeSet::<K>::init_sqlite_tables(db_tx)?;
        bdk_chain::local_chain::ChangeSet::init_sqlite_tables(db_tx)?;
        bdk_chain::tx_graph::ChangeSet::<ConfirmationBlockTime>::init_sqlite_tables(db_tx)?;
        bdk_chain::keychain_txout::ChangeSet::init_sqlite_tables(db_tx)?;

        Ok(())
    }

    /// Recover a [`ChangeSet`] from sqlite database.
    pub fn from_sqlite(db_tx: &chain::rusqlite::Transaction) -> chain::rusqlite::Result<Self> {
        use bitcoin::{OutPoint, Txid};
        use chain::rusqlite::OptionalExtension;
        use chain::Impl;

        let mut changeset = Self::default();

        // Select locked outpoints.
        let mut stmt = db_tx.prepare(&format!(
            "SELECT txid, vout FROM {}",
            Self::WALLET_OUTPOINT_LOCK_TABLE_NAME,
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, Impl<Txid>>("txid")?,
                row.get::<_, u32>("vout")?,
            ))
        })?;
        let locked_outpoints = &mut changeset.locked_outpoints.outpoints;
        for row in rows {
            let (Impl(txid), vout) = row?;
            let outpoint = OutPoint::new(txid, vout);
            locked_outpoints.insert(outpoint, true);
        }

        changeset.keyring = keyring::changeset::ChangeSet::from_sqlite(db_tx)?;
        changeset.local_chain = local_chain::ChangeSet::from_sqlite(db_tx)?;
        changeset.tx_graph = tx_graph::ChangeSet::<_>::from_sqlite(db_tx)?;
        changeset.indexer = keychain_txout::ChangeSet::from_sqlite(db_tx)?;

        Ok(changeset)
    }

    /// Persist [`ChangeSet`] to sqlite database.
    pub fn persist_to_sqlite(
        &self,
        db_tx: &chain::rusqlite::Transaction,
    ) -> chain::rusqlite::Result<()> {
        use chain::rusqlite::named_params;
        use chain::Impl;

        // Insert or delete locked outpoints.
        let mut insert_stmt = db_tx.prepare_cached(&format!(
            "INSERT OR IGNORE INTO {}(txid, vout) VALUES(:txid, :vout)",
            Self::WALLET_OUTPOINT_LOCK_TABLE_NAME
        ))?;
        let mut delete_stmt = db_tx.prepare_cached(&format!(
            "DELETE FROM {} WHERE txid=:txid AND vout=:vout",
            Self::WALLET_OUTPOINT_LOCK_TABLE_NAME,
        ))?;
        for (&outpoint, &is_locked) in &self.locked_outpoints.outpoints {
            let bitcoin::OutPoint { txid, vout } = outpoint;
            if is_locked {
                insert_stmt.execute(named_params! {
                    ":txid": Impl(txid),
                    ":vout": vout,
                })?;
            } else {
                delete_stmt.execute(named_params! {
                    ":txid": Impl(txid),
                    ":vout": vout,
                })?;
            }
        }

        self.keyring.persist_to_sqlite(db_tx)?;
        self.local_chain.persist_to_sqlite(db_tx)?;
        self.tx_graph.persist_to_sqlite(db_tx)?;
        self.indexer.persist_to_sqlite(db_tx)?;
        Ok(())
    }

    /// Initializes the [`ChangeSet`] from persistence.
    ///
    /// Returns Ok(None) if [`ChangeSet`] is empty.
    pub fn initialize(db_tx: &rusqlite::Transaction) -> rusqlite::Result<Option<Self>> {
        Self::init_sqlite_tables(db_tx)?;
        let changeset = Self::from_sqlite(db_tx)?;

        if changeset.is_empty() {
            return Ok(None);
        }
        Ok(Some(changeset))
    }
}

impl<K: Ord> From<local_chain::ChangeSet> for ChangeSet<K> {
    fn from(chain: local_chain::ChangeSet) -> Self {
        Self {
            local_chain: chain,
            ..Default::default()
        }
    }
}

impl<K: Ord> From<IndexedTxGraphChangeSet> for ChangeSet<K> {
    fn from(indexed_tx_graph: IndexedTxGraphChangeSet) -> Self {
        Self {
            tx_graph: indexed_tx_graph.tx_graph,
            indexer: indexed_tx_graph.indexer,
            ..Default::default()
        }
    }
}

impl<K: Ord> From<tx_graph::ChangeSet<ConfirmationBlockTime>> for ChangeSet<K> {
    fn from(tx_graph: tx_graph::ChangeSet<ConfirmationBlockTime>) -> Self {
        Self {
            tx_graph,
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

impl<K> From<locked_outpoints::ChangeSet> for ChangeSet<K>
where
    K: Ord,
{
    fn from(locked_outpoints: locked_outpoints::ChangeSet) -> Self {
        Self {
            locked_outpoints,
            ..Default::default()
        }
    }
}

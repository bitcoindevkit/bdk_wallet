use alloc::collections::btree_map::BTreeMap;
use bdk_chain::{
    indexed_tx_graph, keychain_txout, local_chain, tx_graph, ConfirmationBlockTime, Merge,
};
use miniscript::{Descriptor, DescriptorPublicKey};
use serde::{Deserialize, Serialize};

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

#[cfg(feature = "rusqlite")]
use crate::CanBePersisted;

/// A change set for [`Wallet`]
///
/// ## Definition
///
/// The change set is responsible for transmiting data between the persistent storage layer and the
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
/// in the aggregate. However in practice they should contain the data needed to recover a wallet
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
        }
    }
}

impl<K> Merge for ChangeSet<K>
where
    K: Ord,
{
    /// Merge another [`ChangeSet`] into itself.
    fn merge(&mut self, other: Self) {
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
    }
}

#[cfg(feature = "rusqlite")]
impl<K> ChangeSet<K>
where
    K: Ord + Clone + CanBePersisted,
{
    /// Initialize sqlite tables for wallet tables.
    pub fn init_sqlite_tables(db_tx: &chain::rusqlite::Transaction) -> chain::rusqlite::Result<()> {
        keyring::changeset::ChangeSet::<K>::init_sqlite_tables(db_tx)?;
        bdk_chain::local_chain::ChangeSet::init_sqlite_tables(db_tx)?;
        bdk_chain::tx_graph::ChangeSet::<ConfirmationBlockTime>::init_sqlite_tables(db_tx)?;
        bdk_chain::keychain_txout::ChangeSet::init_sqlite_tables(db_tx)?;

        Ok(())
    }

    /// Recover a [`ChangeSet`] from sqlite database.
    pub fn from_sqlite(db_tx: &chain::rusqlite::Transaction) -> chain::rusqlite::Result<Self> {
        let mut changeset = Self::default();
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

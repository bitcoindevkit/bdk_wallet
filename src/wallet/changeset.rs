use crate::collections::BTreeMap;
use alloc::collections::btree_map::Entry;
use bdk_chain::{
    ConfirmationBlockTime, Merge, indexed_tx_graph, keychain_txout, local_chain, tx_graph,
};
use miniscript::{Descriptor, DescriptorPublicKey};
use serde::{Deserialize, Serialize};

use crate::locked_outpoints;

type IndexedTxGraphChangeSet =
    indexed_tx_graph::ChangeSet<ConfirmationBlockTime, keychain_txout::ChangeSet>;

/// A change set for [`Wallet`].
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
/// These include the [`descriptors`] and the [`bitcoin::Network`] in use. These are required to be
/// non-empty *in the aggregate*, meaning the field must be present and non-null in the union of all
/// persisted changes, but may be empty in any one change set, where "empty" is defined by the
/// [`Merge`](Merge::is_empty) implementation of that change set. This requirement also applies to
/// the [`local_chain`] field in that the aggregate change set must include a genesis block.
///
/// For example, the [`descriptors`] and [`bitcoin::Network`] are present in the first change set
/// after wallet creation, but are usually omitted in subsequent updates, as they are not permitted
/// to change at any point thereafter.
///
/// Other fields of the change set are not required to be non-empty, that is they may be empty even
/// in the aggregate. However, in practice they should contain the data needed to recover a wallet
/// state between sessions. These include:
/// * [`tx_graph`](Self::tx_graph)
/// * [`indexer`](Self::indexer)
///
/// A keychain may be introduced by a later change set — a wallet can start tracking a new one at
/// any time — but the descriptor bound to a keychain is fixed at the point that keychain first
/// appears, and must be identical in every change set thereafter.
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
/// be removed in future versions of the library following the deprecation policy below.
///
/// ## Version Compatibility
///
/// Any change to the [`ChangeSet`] data structure MUST correlate with a major version bump per
/// [Semantic Versioning]. We guarantee that version N can read and
/// deserialize [`ChangeSet`] data written by version N-1 (one major version back), but this
/// guarantee does NOT extend to version N-2 or earlier. New fields added in version N must
/// implement [`Default`] so that when reading N-1 data, absent fields are populated with default
/// values.
///
/// Limited forward compatibility is provided for downgrades: version N-1 will successfully
/// deserialize version N data without errors by ignoring unknown fields. Users should be aware that
/// features introduced in version N will not be available when downgrading to N-1, and that
/// downgrading can result in loss of data if not backed up. For this reason we recommend carefully
/// planning major upgrades and backing up necessary data to avoid compatibility issues.
///
/// Fields can be removed using a 3-version deprecation cycle: fields are marked deprecated in
/// version N with a reason and instructions for migrating, the field is retained in version N+1
/// for compatibility where it deserializes but may not be used, and finally removed in version
/// N+2. This ensures the standard backwards compatibility guarantees while allowing the removal of
/// deprecated fields.
///
/// ### Responsibilities
///
/// Library authors SHOULD test all upgrade paths using the persistence test suite and in CI.
/// Library authors MUST document API changes prominently in the release notes and CHANGELOG,
/// clearly mark deprecated fields including migration instructions, and follow the 3-version
/// deprecation cycle before removing fields.
///
/// Users SHOULD back up wallet data before major version upgrades, test upgrades in non-production
/// environments first, and monitor the release notes for warnings and updates. Users MUST complete
/// migrations within the compatibility window, and not skip major versions (i.e. upgrade major
/// versions sequentially).
///
/// ### Custom Persistence Implementations
///
/// The resulting interface is designed to give the user more control of what to persist and when
/// to persist it. Custom implementations should consider and account for the possibility of
/// partial or repeat writes, the atomicity of persistence operations, and the order of reads and
/// writes among the fields of the change set. BDK comes with support for [SQLite] that handles
/// the details for you and is recommended for many users. If implementing your own persistence,
/// please refer to the documentation for [`WalletPersister`] and [`PersistedWallet`] for more
/// information.
///
/// [`descriptors`]: Self::descriptors
/// [`local_chain`]: Self::local_chain
/// [merged]: bdk_chain::Merge
/// [`network`]: Self::network
/// [`PersistedWallet`]: crate::PersistedWallet
/// [SQLite]: <https://docs.rs/rusqlite/0.31.0/rusqlite/>
/// [`Update`]: crate::Update
/// [`WalletPersister`]: crate::WalletPersister
/// [`Wallet::staged`]: crate::Wallet::staged
/// [`Wallet`]: crate::Wallet
/// [Semantic Versioning]: <https://doc.rust-lang.org/cargo/reference/semver.html>
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ChangeSet<K: Ord> {
    /// The descriptor tracked by each keychain.
    ///
    /// A keychain's descriptor is fixed for the life of the wallet: [`Merge`] will accept a
    /// keychain it has not seen before, but never a different descriptor for one it has.
    pub descriptors: BTreeMap<K, Descriptor<DescriptorPublicKey>>,
    /// Stores the network type of the transaction data.
    pub network: Option<bitcoin::Network>,
    /// Changes to the [`LocalChain`](local_chain::LocalChain).
    pub local_chain: local_chain::ChangeSet,
    /// Changes to [`TxGraph`](tx_graph::TxGraph).
    pub tx_graph: tx_graph::ChangeSet<ConfirmationBlockTime>,
    /// Changes to [`KeychainTxOutIndex`](keychain_txout::KeychainTxOutIndex).
    pub indexer: keychain_txout::ChangeSet,
    /// Changes to locked outpoints.
    #[serde(default)]
    pub locked_outpoints: locked_outpoints::ChangeSet,
}

impl<K: Ord> Default for ChangeSet<K> {
    fn default() -> Self {
        Self {
            descriptors: BTreeMap::new(),
            network: None,
            local_chain: Default::default(),
            tx_graph: Default::default(),
            indexer: Default::default(),
            locked_outpoints: Default::default(),
        }
    }
}

impl<K: Ord> Merge for ChangeSet<K> {
    /// Merge another [`ChangeSet`] into itself.
    fn merge(&mut self, other: Self) {
        // A keychain may be introduced by a later changeset, but the descriptor it is bound to
        // must never change. `extend` would silently overwrite, which would let a corrupt or
        // hostile changeset swap a descriptor out from under a loaded wallet.
        for (keychain, descriptor) in other.descriptors {
            match self.descriptors.entry(keychain) {
                Entry::Vacant(slot) => {
                    slot.insert(descriptor);
                }
                Entry::Occupied(slot) => {
                    debug_assert!(
                        *slot.get() == descriptor,
                        "a keychain's descriptor must never change"
                    );
                }
            }
        }
        if self.network.is_none() && other.network.is_some() {
            self.network = other.network;
        } else {
            debug_assert!(
                other.network.is_none() || self.network == other.network,
                "network must never change"
            );
        }

        // merge locked outpoints
        self.locked_outpoints.merge(other.locked_outpoints);

        Merge::merge(&mut self.local_chain, other.local_chain);
        Merge::merge(&mut self.tx_graph, other.tx_graph);
        Merge::merge(&mut self.indexer, other.indexer);
    }

    fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
            && self.network.is_none()
            && self.local_chain.is_empty()
            && self.tx_graph.is_empty()
            && self.indexer.is_empty()
            && self.locked_outpoints.is_empty()
    }
}

#[cfg(feature = "rusqlite")]
impl<K> ChangeSet<K>
where
    K: Ord + Clone + chain::rusqlite::ToSql + chain::rusqlite::types::FromSql,
{
    /// Schema name for wallet.
    pub const WALLET_SCHEMA_NAME: &'static str = "bdk_wallet";
    /// Name of table to store wallet descriptors and network.
    pub const WALLET_TABLE_NAME: &'static str = "bdk_wallet";
    /// Name of table to store wallet locked outpoints.
    pub const WALLET_OUTPOINT_LOCK_TABLE_NAME: &'static str = "bdk_wallet_locked_outpoints";

    /// Get v0 sqlite [ChangeSet] schema
    pub fn schema_v0() -> alloc::string::String {
        format!(
            "CREATE TABLE {} ( \
                id INTEGER PRIMARY KEY NOT NULL CHECK (id = 0), \
                descriptor TEXT, \
                change_descriptor TEXT, \
                network TEXT \
                ) STRICT;",
            Self::WALLET_TABLE_NAME,
        )
    }

    /// Get v1 sqlite [`ChangeSet`] schema. Schema v1 adds a table for locked outpoints.
    pub fn schema_v1() -> alloc::string::String {
        format!(
            "CREATE TABLE {} ( \
                txid TEXT NOT NULL, \
                vout INTEGER NOT NULL, \
                PRIMARY KEY(txid, vout) \
                ) STRICT;",
            Self::WALLET_OUTPOINT_LOCK_TABLE_NAME,
        )
    }

    /// Name of table storing one descriptor per keychain.
    pub const WALLET_KEYCHAIN_TABLE_NAME: &'static str = "bdk_wallet_keychain";

    /// Get v2 sqlite [`ChangeSet`] schema.
    ///
    /// Schema v2 replaces the single-row `descriptor` / `change_descriptor` columns with one row
    /// per keychain, so a wallet may track any number of them.
    pub fn schema_v2() -> alloc::string::String {
        format!(
            "CREATE TABLE {} ( \
                keychain TEXT PRIMARY KEY NOT NULL, \
                descriptor TEXT NOT NULL \
                ) STRICT;",
            Self::WALLET_KEYCHAIN_TABLE_NAME,
        )
    }

    /// Initialize sqlite tables for wallet tables.
    pub fn init_sqlite_tables(db_tx: &chain::rusqlite::Transaction) -> chain::rusqlite::Result<()> {
        crate::rusqlite_impl::migrate_schema(
            db_tx,
            Self::WALLET_SCHEMA_NAME,
            &[&Self::schema_v0(), &Self::schema_v1(), &Self::schema_v2()],
        )?;

        bdk_chain::local_chain::ChangeSet::init_sqlite_tables(db_tx)?;
        bdk_chain::tx_graph::ChangeSet::<ConfirmationBlockTime>::init_sqlite_tables(db_tx)?;
        bdk_chain::keychain_txout::ChangeSet::init_sqlite_tables(db_tx)?;

        Ok(())
    }

    /// Recover a [`ChangeSet`] from sqlite database.
    pub fn from_sqlite(db_tx: &chain::rusqlite::Transaction) -> chain::rusqlite::Result<Self> {
        use bitcoin::{OutPoint, Txid};
        use chain::Impl;
        use chain::rusqlite::OptionalExtension;

        let mut changeset = Self::default();

        let mut network_statement =
            db_tx.prepare(&format!("SELECT network FROM {}", Self::WALLET_TABLE_NAME,))?;
        let row = network_statement
            .query_row([], |row| {
                row.get::<_, Option<Impl<bitcoin::Network>>>("network")
            })
            .optional()?;
        if let Some(network) = row.flatten() {
            changeset.network = Some(network.into_inner());
        }

        let mut keychain_statement = db_tx.prepare(&format!(
            "SELECT keychain, descriptor FROM {}",
            Self::WALLET_KEYCHAIN_TABLE_NAME,
        ))?;
        let rows = keychain_statement.query_map([], |row| {
            Ok((
                row.get::<_, K>("keychain")?,
                row.get::<_, Impl<Descriptor<DescriptorPublicKey>>>("descriptor")?,
            ))
        })?;
        for row in rows {
            let (keychain, Impl(descriptor)) = row?;
            changeset.descriptors.insert(keychain, descriptor);
        }

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
        use chain::Impl;
        use chain::rusqlite::named_params;

        // A keychain's descriptor never changes once written, so first write wins.
        let mut keychain_statement = db_tx.prepare_cached(&format!(
            "INSERT OR IGNORE INTO {}(keychain, descriptor) VALUES(:keychain, :descriptor)",
            Self::WALLET_KEYCHAIN_TABLE_NAME,
        ))?;
        for (keychain, descriptor) in &self.descriptors {
            keychain_statement.execute(named_params! {
                ":keychain": keychain,
                ":descriptor": Impl(descriptor.clone()),
            })?;
        }

        let mut network_statement = db_tx.prepare_cached(&format!(
            "INSERT INTO {}(id, network) VALUES(:id, :network) ON CONFLICT(id) DO UPDATE SET network=COALESCE({}.network, :network)",
            Self::WALLET_TABLE_NAME,
            Self::WALLET_TABLE_NAME,
        ))?;
        if let Some(network) = self.network {
            network_statement.execute(named_params! {
                ":id": 0,
                ":network": Impl(network),
            })?;
        }

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

        self.local_chain.persist_to_sqlite(db_tx)?;
        self.tx_graph.persist_to_sqlite(db_tx)?;
        self.indexer.persist_to_sqlite(db_tx)?;
        Ok(())
    }
}

#[cfg(feature = "rusqlite")]
impl ChangeSet<crate::KeychainKind> {
    /// Recover descriptors written by schema v0 or v1.
    ///
    /// Those versions stored the wallet's two descriptors as `descriptor` and `change_descriptor`
    /// columns on a single row, rather than one row per keychain. This reads them and maps them
    /// onto [`External`](crate::KeychainKind::External) and
    /// [`Internal`](crate::KeychainKind::Internal).
    ///
    /// Only meaningful for wallets keyed by [`KeychainKind`](crate::KeychainKind): interpreting
    /// the two legacy columns *requires* knowing they mean external and change. A wallet using a
    /// custom keychain type has no v0/v1 database to recover, since those schemas predate custom
    /// keychains entirely.
    ///
    /// Descriptors already present are left alone, so this never overwrites what schema v2 holds.
    pub fn read_legacy_descriptors(
        db_tx: &chain::rusqlite::Transaction,
        changeset: &mut Self,
    ) -> chain::rusqlite::Result<()> {
        use crate::KeychainKind;
        use chain::Impl;
        use chain::rusqlite::OptionalExtension;

        let mut statement = db_tx.prepare(&format!(
            "SELECT descriptor, change_descriptor FROM {}",
            Self::WALLET_TABLE_NAME,
        ))?;
        let row = statement
            .query_row([], |row| {
                Ok((
                    row.get::<_, Option<Impl<Descriptor<DescriptorPublicKey>>>>("descriptor")?,
                    row.get::<_, Option<Impl<Descriptor<DescriptorPublicKey>>>>(
                        "change_descriptor",
                    )?,
                ))
            })
            .optional()?;

        let Some((descriptor, change_descriptor)) = row else {
            return Ok(());
        };

        if let Some(Impl(descriptor)) = descriptor {
            changeset
                .descriptors
                .entry(KeychainKind::External)
                .or_insert(descriptor);
        }
        if let Some(Impl(change_descriptor)) = change_descriptor {
            changeset
                .descriptors
                .entry(KeychainKind::Internal)
                .or_insert(change_descriptor);
        }

        Ok(())
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

impl<K: Ord> From<locked_outpoints::ChangeSet> for ChangeSet<K> {
    fn from(locked_outpoints: locked_outpoints::ChangeSet) -> Self {
        Self {
            locked_outpoints,
            ..Default::default()
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod test {
    use super::*;

    // Tests that merging `ChangeSet`s with write-once fields follows "first write wins" semantics
    //
    // Verifies three scenarios:
    // 1. `None` + `Some(x)` => `Some(x)` (initial write accepted)
    // 2. `Some(x)` + `None` => `Some(x)` (field is not cleared)
    // 3. `Some(x)` + `Some(y)` => `Some(x)` (same value, no change)
    #[cfg(not(debug_assertions))]
    #[test]
    fn merge_first_write_wins() {
        use super::*;
        use crate::persist_test_utils::DESCRIPTORS;
        use bitcoin::Network;
        let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();
        let change_descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[1].parse().unwrap();

        // Scenario 1: None + Some(x) - first write populates the field
        let mut change_set = ChangeSet::default();
        let other_change_set = ChangeSet {
            descriptor: Some(descriptor.clone()),
            change_descriptor: Some(change_descriptor.clone()),
            network: Some(Network::Bitcoin),
            ..ChangeSet::default()
        };
        Merge::merge(&mut change_set, other_change_set);
        assert_eq!(
            change_set.descriptor,
            Some(descriptor.clone()),
            "descriptor should be populated from first merge"
        );
        assert_eq!(
            change_set.change_descriptor,
            Some(change_descriptor.clone()),
            "change_descriptor should be populated from first merge"
        );
        assert_eq!(
            change_set.network,
            Some(Network::Bitcoin),
            "network should be populated from first merge"
        );

        // Scenario 2: Some(x) + None - existing field is unchanged
        let mut change_set = ChangeSet {
            descriptor: Some(descriptor.clone()),
            change_descriptor: Some(change_descriptor.clone()),
            network: Some(Network::Bitcoin),
            ..ChangeSet::default()
        };
        Merge::merge(&mut change_set, ChangeSet::default());
        assert_eq!(
            change_set.descriptor,
            Some(descriptor.clone()),
            "descriptor must not change when merging empty changeset"
        );
        assert_eq!(
            change_set.change_descriptor,
            Some(change_descriptor.clone()),
            "change_descriptor must not change when merging empty changeset"
        );
        assert_eq!(
            change_set.network,
            Some(Network::Bitcoin),
            "network must not change when merging empty changeset"
        );

        // Scenario 3: Some(x) + Some(y) - existing field is unchanged
        let mut change_set = ChangeSet {
            descriptor: Some(descriptor.clone()),
            change_descriptor: Some(change_descriptor.clone()),
            network: Some(Network::Bitcoin),
            ..ChangeSet::default()
        };
        let other_descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[2].parse().unwrap();
        let other_change_descriptor: Descriptor<DescriptorPublicKey> =
            DESCRIPTORS[3].parse().unwrap();
        let other_change_set = ChangeSet {
            descriptor: Some(other_descriptor),
            change_descriptor: Some(other_change_descriptor),
            network: Some(Network::Regtest),
            ..ChangeSet::default()
        };
        assert_ne!(change_set, other_change_set);
        Merge::merge(&mut change_set, other_change_set);
        assert_eq!(
            change_set.descriptor,
            Some(descriptor),
            "descriptor must not change when merging other value"
        );
        assert_eq!(
            change_set.change_descriptor,
            Some(change_descriptor),
            "change_descriptor must not change when merging other value"
        );
        assert_eq!(
            change_set.network,
            Some(Network::Bitcoin),
            "network must not change when merging other value"
        );
    }

    #[cfg(feature = "rusqlite")]
    #[test]
    fn reads_descriptors_from_a_legacy_v0_database() {
        use crate::KeychainKind;
        use bitcoin::Network;
        use chain::rusqlite::{Connection, named_params};

        const EXTERNAL: &str = "wpkh([41f2aed0/84h/1h/0h]tpubDDFSdQWw75hk1ewbwnNpPp5DvXFRKt68ioPoyJDY752cNHKkFxPWqkqCyCf4hxrEfpuxh46QisehL3m8Bi6MsAv394QVLopwbtfvryFQNUH/0/*)";
        const INTERNAL: &str = "wpkh([41f2aed0/84h/1h/0h]tpubDDFSdQWw75hk1ewbwnNpPp5DvXFRKt68ioPoyJDY752cNHKkFxPWqkqCyCf4hxrEfpuxh46QisehL3m8Bi6MsAv394QVLopwbtfvryFQNUH/1/*)";

        let mut conn = Connection::open_in_memory().unwrap();
        let db_tx = conn.transaction().unwrap();
        ChangeSet::<KeychainKind>::init_sqlite_tables(&db_tx).unwrap();

        // Simulate a wallet written before schema v2: descriptors live in the columns on the
        // single wallet row, and the per-keychain table is empty.
        let external: Descriptor<DescriptorPublicKey> = EXTERNAL.parse().unwrap();
        let internal: Descriptor<DescriptorPublicKey> = INTERNAL.parse().unwrap();
        db_tx
            .execute(
                &format!(
                    "INSERT INTO {}(id, descriptor, change_descriptor, network) \
                     VALUES(:id, :descriptor, :change_descriptor, :network)",
                    ChangeSet::<KeychainKind>::WALLET_TABLE_NAME
                ),
                named_params! {
                    ":id": 0,
                    ":descriptor": chain::Impl(external.clone()),
                    ":change_descriptor": chain::Impl(internal.clone()),
                    ":network": chain::Impl(Network::Testnet),
                },
            )
            .unwrap();

        let mut changeset = ChangeSet::<KeychainKind>::from_sqlite(&db_tx).unwrap();
        // Nothing in the v2 table yet.
        assert!(changeset.descriptors.is_empty());
        assert_eq!(changeset.network, Some(Network::Testnet));

        ChangeSet::<KeychainKind>::read_legacy_descriptors(&db_tx, &mut changeset).unwrap();

        assert_eq!(
            changeset.descriptors.get(&KeychainKind::External),
            Some(&external),
            "legacy `descriptor` column must map to the external keychain"
        );
        assert_eq!(
            changeset.descriptors.get(&KeychainKind::Internal),
            Some(&internal),
            "legacy `change_descriptor` column must map to the internal keychain"
        );
    }

    #[cfg(feature = "rusqlite")]
    #[test]
    fn legacy_read_never_overwrites_v2_descriptors() {
        use crate::KeychainKind;
        use chain::rusqlite::{Connection, named_params};

        const V2_DESC: &str = "wpkh([41f2aed0/84h/1h/0h]tpubDDFSdQWw75hk1ewbwnNpPp5DvXFRKt68ioPoyJDY752cNHKkFxPWqkqCyCf4hxrEfpuxh46QisehL3m8Bi6MsAv394QVLopwbtfvryFQNUH/0/*)";
        const LEGACY_DESC: &str = "wpkh([41f2aed0/84h/1h/0h]tpubDDFSdQWw75hk1ewbwnNpPp5DvXFRKt68ioPoyJDY752cNHKkFxPWqkqCyCf4hxrEfpuxh46QisehL3m8Bi6MsAv394QVLopwbtfvryFQNUH/1/*)";

        let mut conn = Connection::open_in_memory().unwrap();
        let db_tx = conn.transaction().unwrap();
        ChangeSet::<KeychainKind>::init_sqlite_tables(&db_tx).unwrap();

        let v2: Descriptor<DescriptorPublicKey> = V2_DESC.parse().unwrap();
        let legacy: Descriptor<DescriptorPublicKey> = LEGACY_DESC.parse().unwrap();

        // A stale legacy column alongside an authoritative v2 row for the same keychain.
        db_tx
            .execute(
                &format!(
                    "INSERT INTO {}(id, descriptor) VALUES(:id, :descriptor)",
                    ChangeSet::<KeychainKind>::WALLET_TABLE_NAME
                ),
                named_params! { ":id": 0, ":descriptor": chain::Impl(legacy) },
            )
            .unwrap();

        let mut changeset = ChangeSet::<KeychainKind>::default();
        changeset
            .descriptors
            .insert(KeychainKind::External, v2.clone());

        ChangeSet::<KeychainKind>::read_legacy_descriptors(&db_tx, &mut changeset).unwrap();

        assert_eq!(
            changeset.descriptors.get(&KeychainKind::External),
            Some(&v2),
            "schema v2 is authoritative; a legacy column must not overwrite it"
        );
    }
}

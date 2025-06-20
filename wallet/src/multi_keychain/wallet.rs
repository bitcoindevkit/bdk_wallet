use core::fmt;

use bitcoin::Address;
use miniscript::{Descriptor, DescriptorPublicKey};

#[cfg(feature = "rusqlite")]
use bdk_chain::rusqlite;
use bdk_chain::{
    keychain_txout::{KeychainTxOutIndex, DEFAULT_LOOKAHEAD},
    local_chain::LocalChain,
    CheckPoint, ConfirmationBlockTime, IndexedTxGraph, KeychainIndexed, Merge,
};

use crate::collections::BTreeMap;
use crate::multi_keychain::{ChangeSet, Did, KeyRing};

/// Alias for a [`IndexedTxGraph`].
type KeychainTxGraph<K> = IndexedTxGraph<ConfirmationBlockTime, KeychainTxOutIndex<K>>;

// This is here for dev purposes and can be made a configurable option as part of the final API.
const USE_SPK_CACHE: bool = false;

/// [`Wallet`] is a structure that stores transaction data that can be indexed by multiple
/// keychains.
#[derive(Debug)]
pub struct Wallet<K: Ord> {
    keyring: KeyRing<K>,
    chain: LocalChain,
    tx_graph: KeychainTxGraph<K>,
    stage: ChangeSet<K>,
}

impl<K> Wallet<K>
where
    K: fmt::Debug + Clone + Ord,
{
    /// Construct a new [`Wallet`] with the given `keyring`.
    pub fn new(mut keyring: KeyRing<K>) -> Self {
        let network = keyring.network;

        let genesis_hash = bitcoin::constants::genesis_block(network).block_hash();
        let (chain, chain_changeset) = LocalChain::from_genesis_hash(genesis_hash);

        let keyring_changeset = keyring.initial_changeset();

        let mut index = KeychainTxOutIndex::new(DEFAULT_LOOKAHEAD, USE_SPK_CACHE);
        let descriptors = core::mem::take(&mut keyring.descriptors);
        for (keychain, desc) in descriptors {
            let _inserted = index
                .insert_descriptor(keychain, desc)
                .expect("err: failed to insert descriptor");
            assert!(_inserted);
        }

        let tx_graph = KeychainTxGraph::new(index);

        let stage = ChangeSet {
            keyring: keyring_changeset,
            local_chain: chain_changeset,
            tx_graph: bdk_chain::tx_graph::ChangeSet::default(),
            indexer: bdk_chain::keychain_txout::ChangeSet::default(),
        };

        Self {
            keyring,
            chain,
            tx_graph,
            stage,
        }
    }

    /// Construct [`Wallet`] from the provided `changeset`.
    ///
    /// Will be `None` if the changeset is empty.
    pub fn from_changeset(changeset: ChangeSet<K>) -> Option<Self> {
        if changeset.is_empty() {
            return None;
        }

        // chain
        let chain =
            LocalChain::from_changeset(changeset.local_chain).expect("err: Missing genesis");

        // keyring
        let mut keyring = KeyRing::from_changeset(changeset.keyring)?;

        // index
        let mut index = KeychainTxOutIndex::new(DEFAULT_LOOKAHEAD, USE_SPK_CACHE);
        index.apply_changeset(changeset.indexer);
        for (keychain, descriptor) in core::mem::take(&mut keyring.descriptors) {
            let _inserted = index
                .insert_descriptor(keychain, descriptor)
                .expect("failed to insert descriptor");
            assert!(_inserted);
        }

        // txgraph
        let mut tx_graph = KeychainTxGraph::new(index);
        tx_graph.apply_changeset(changeset.tx_graph.into());

        let stage = ChangeSet::default();

        Some(Self {
            tx_graph,
            stage,
            chain,
            keyring,
        })
    }

    /// Reveal next default address. Panics if the default implementation of `K` does not match
    /// a keychain contained in this wallet.
    pub fn reveal_next_default_address_unwrap(&mut self) -> KeychainIndexed<K, Address>
    where
        K: Default,
    {
        self.reveal_next_address(K::default())
            .expect("invalid keychain")
    }

    /// Reveal next address from the given `keychain`.
    ///
    /// This may return the last revealed address in case there are none left to reveal.
    pub fn reveal_next_address(&mut self, keychain: K) -> Option<KeychainIndexed<K, Address>> {
        let ((index, spk), index_changeset) =
            self.tx_graph.index.reveal_next_spk(keychain.clone())?;
        let address = Address::from_script(&spk, self.keyring.network)
            .expect("script should have address form");

        self.stage(index_changeset);

        Some(((keychain, index), address))
    }

    /// Iterate over `(keychain descriptor)` pairs contained in this wallet.
    pub fn keychains(
        &self,
    ) -> impl DoubleEndedIterator<Item = (K, &Descriptor<DescriptorPublicKey>)> {
        self.tx_graph.index.keychains()
    }

    /// Compute the balance.
    pub fn balance(&self) -> bdk_chain::Balance {
        use bdk_chain::CanonicalizationParams;
        let chain = &self.chain;
        let outpoints = self.tx_graph.index.outpoints().clone();
        self.tx_graph.graph().balance(
            chain,
            chain.tip().block_id(),
            CanonicalizationParams::default(),
            outpoints,
            |_, _| false,
        )
    }

    /// Obtain a reference to the indexed transaction graph.
    pub fn tx_graph(&self) -> &KeychainTxGraph<K> {
        &self.tx_graph
    }

    /// Obtain a reference to the keychain indexer.
    pub fn index(&self) -> &KeychainTxOutIndex<K> {
        &self.tx_graph.index
    }

    /// Obtain a reference to the local chain.
    pub fn local_chain(&self) -> &LocalChain {
        &self.chain
    }

    /// Apply update.
    pub fn apply_update(&mut self, update: impl Into<Update<K>>) {
        let Update {
            chain,
            tx_update,
            last_active_indices,
        } = update.into();

        let mut changeset = ChangeSet::default();

        // chain
        if let Some(tip) = chain {
            changeset.merge(
                self.chain
                    .apply_update(tip)
                    .expect("err: failed to apply update to chain")
                    .into(),
            );
        }
        // index
        changeset.merge(
            self.tx_graph
                .index
                .reveal_to_target_multi(&last_active_indices)
                .into(),
        );
        // tx graph
        changeset.merge(self.tx_graph.apply_update(tx_update).into());

        self.stage(changeset);
    }

    /// Stages anything that can be converted directly into a [`ChangeSet`].
    fn stage(&mut self, changeset: impl Into<ChangeSet<K>>) {
        self.stage.merge(changeset.into());
    }

    /// See the staged changes if any.
    pub fn staged(&self) -> Option<&ChangeSet<K>> {
        if self.stage.is_empty() {
            None
        } else {
            Some(&self.stage)
        }
    }
}

// TODO: This should probably be handled by `PersistedWallet` or similar
#[cfg(feature = "rusqlite")]
impl Wallet<Did> {
    /// Construct [`Wallet`] from SQLite.
    pub fn from_sqlite(conn: &mut rusqlite::Connection) -> rusqlite::Result<Option<Self>> {
        let tx = conn.transaction()?;

        let changeset = ChangeSet::initialize(&tx)?;
        tx.commit()?;

        Ok(changeset.and_then(Self::from_changeset))
    }

    /// Persist to SQLite. Returns the newly committed changeset if successful, or `None`
    /// if the stage is currently empty.
    pub fn persist_to_sqlite(
        &mut self,
        conn: &mut rusqlite::Connection,
    ) -> rusqlite::Result<Option<ChangeSet<Did>>> {
        let mut ret = None;

        let tx = conn.transaction()?;

        if let Some(changeset) = self.staged_changeset() {
            changeset.persist_to_sqlite(&tx)?;
            tx.commit()?;
            ret = self.stage.take();
        }

        Ok(ret)
    }

    /// See the staged changes if any.
    pub fn staged_changeset(&self) -> Option<&ChangeSet<Did>> {
        if self.stage.is_empty() {
            None
        } else {
            Some(&self.stage)
        }
    }
}

/// Contains structures for updating a multi-keychain wallet.
#[derive(Debug)]
pub struct Update<K> {
    /// chain
    pub chain: Option<CheckPoint>,
    /// tx update
    pub tx_update: bdk_chain::TxUpdate<ConfirmationBlockTime>,
    /// last active keychain indices
    pub last_active_indices: BTreeMap<K, u32>,
}

impl<K> From<bdk_chain::spk_client::FullScanResponse<K>> for Update<K> {
    fn from(resp: bdk_chain::spk_client::FullScanResponse<K>) -> Self {
        Self {
            chain: resp.chain_update,
            tx_update: resp.tx_update,
            last_active_indices: resp.last_active_indices,
        }
    }
}

// Bitcoin Dev Kit
// Written in 2020 by Alekos Filini <alekos.filini@gmail.com>
//
// Copyright (c) 2020-2025 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

//! Wallet
//!
//! This module defines the [`Wallet`].
#![allow(unused)]
use alloc::{
    boxed::Box,
    collections::BTreeSet,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::{
    cmp::Ordering,
    fmt::{self, Debug, Display},
    mem,
    ops::Deref,
};

use bdk_chain::{
    indexed_tx_graph,
    indexer::keychain_txout::{
        self, FullScanRequestBuilderExt, InsertDescriptorError, KeychainTxOutIndex,
        SyncRequestBuilderExt, DEFAULT_LOOKAHEAD,
    },
    local_chain::{ApplyHeaderError, CannotConnectError, CheckPoint, CheckPointIter, LocalChain},
    spk_client::{
        FullScanRequest, FullScanRequestBuilder, FullScanResponse, SyncRequest, SyncRequestBuilder,
        SyncResponse,
    },
    tx_graph::{self, CalculateFeeError, CanonicalTx, TxGraph, TxUpdate},
    Anchor, BlockId, CanonicalizationParams, ChainPosition, ConfirmationBlockTime, DescriptorExt,
    FullTxOut, Indexed, IndexedTxGraph, Indexer, Merge,
};
use bitcoin::{
    absolute,
    consensus::encode::serialize,
    constants::genesis_block,
    psbt,
    secp256k1::Secp256k1,
    sighash::{EcdsaSighashType, TapSighashType},
    transaction, Address, Amount, Block, FeeRate, Network, NetworkKind, OutPoint, Psbt, ScriptBuf,
    Sequence, SignedAmount, Transaction, TxOut, Txid, Weight, Witness,
};
use miniscript::{
    descriptor::KeyMap,
    psbt::{PsbtExt, PsbtInputExt, PsbtInputSatisfier},
    Descriptor, DescriptorPublicKey,
};
use rand_core::RngCore;

mod changeset;
pub mod error;
mod event;
// pub mod export;
pub mod locked_outpoints;
mod params;
mod persisted;
pub mod signer;
pub mod tx_builder;
pub(crate) mod utils;

use crate::descriptor::{
    self, check_wallet_descriptor, DerivedDescriptor, DescriptorMeta, ExtendedDescriptor,
    IntoWalletDescriptor, XKeyUtils,
};
use crate::keyring::{KeyRing, KeyRingError};
use crate::psbt::PsbtUtils;
use crate::types::*;
use crate::wallet::{
    error::{
        BuildFeeBumpError,
        // CreateTxError,
        MiniscriptPsbtError,
    },
    signer::{SignOptions, SignerError, SignerOrdering, SignersContainer, TransactionSigner},
    utils::{check_nsequence_rbf, After, Older, SecpCtx},
};
use crate::{
    collections::{BTreeMap, HashMap, HashSet},
    keyring,
};

#[cfg(feature = "rusqlite")]
use bdk_chain::{
    rusqlite::{
        self,
        types::{FromSql, ToSql},
    },
    DescriptorId, Impl,
};

// re-exports
pub use bdk_chain::Balance;
pub use changeset::ChangeSet;
pub use error::LoadError;
pub use event::*;
pub use params::*;
pub use persisted::*;
pub use utils::IsDust;
pub use utils::TxDetails;

/// A Bitcoin wallet
///
/// The `Wallet` acts as a way of coherently interfacing with output descriptors and related
/// transactions. Its main component is a [`KeyRing`] which holds the network and output
/// descriptors.
///
/// The user is responsible for loading and writing wallet changes which are represented as
/// [`ChangeSet`]s (see [`take_staged`]). Also see individual functions and example for instructions
/// on when [`Wallet`] state needs to be persisted.
///
/// The `Wallet` descriptors must not derive the same script pubkeys.
/// See [`KeychainTxOutIndex::insert_descriptor()`] for more details.
///
/// [`take_staged`]: Wallet::take_staged
#[derive(Debug)]
pub struct Wallet<K: Ord> {
    keyring: KeyRing<K>,
    chain: LocalChain,
    tx_graph: IndexedTxGraph<ConfirmationBlockTime, KeychainTxOutIndex<K>>,
    locked_outpoints: HashSet<OutPoint>,
    stage: ChangeSet<K>,
}

/// An update to [`Wallet`].
///
/// It updates [`KeychainTxOutIndex`], [`bdk_chain::TxGraph`] and [`LocalChain`] atomically.
#[derive(Debug, Clone)]
pub struct Update<K> {
    /// Contains the last active derivation indices per keychain (`K`), which is used to update the
    /// [`KeychainTxOutIndex`].
    pub last_active_indices: BTreeMap<K, u32>,

    /// Update for the wallet's internal [`TxGraph`].
    pub tx_update: TxUpdate<ConfirmationBlockTime>,

    /// Update for the wallet's internal [`LocalChain`].
    pub chain: Option<CheckPoint>,
}

impl<K> From<FullScanResponse<K>> for Update<K> {
    fn from(value: FullScanResponse<K>) -> Self {
        Self {
            last_active_indices: value.last_active_indices,
            tx_update: value.tx_update,
            chain: value.chain_update,
        }
    }
}

impl<K> From<SyncResponse> for Update<K> {
    fn from(value: SyncResponse) -> Self {
        Self {
            last_active_indices: BTreeMap::new(),
            tx_update: value.tx_update,
            chain: value.chain_update,
        }
    }
}

impl<K> Default for Update<K> {
    fn default() -> Self {
        Update {
            last_active_indices: Default::default(),
            tx_update: Default::default(),
            chain: Default::default(),
        }
    }
}

/// A derived address and the index it was found at.
/// For convenience this automatically derefs to `Address`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressInfo<K> {
    /// Child index of this address
    pub index: u32,
    /// Address
    pub address: Address,
    /// Type of keychain
    pub keychain: K,
}

impl<K> Deref for AddressInfo<K> {
    type Target = Address;

    fn deref(&self) -> &Self::Target {
        &self.address
    }
}

impl<K> Display for AddressInfo<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.address)
    }
}

/// A `CanonicalTx` managed by a `Wallet`.
pub type WalletTx<'a> = CanonicalTx<'a, Arc<Transaction>, ConfirmationBlockTime>;

// This impl block contains wallet construction associated functions
impl<K> Wallet<K>
where
    K: Clone + Debug + Ord,
{
    /// Construct a new [`Wallet`] with the given `keyring`.
    pub fn create(mut keyring: KeyRing<K>) -> CreateParams<K> {
        CreateParams::new(keyring)
    }

    /// Construct a new [`Wallet`] with the given `params`.
    ///
    /// The `genesis_hash` (if not specified) will be inferred from `keyring.network`.
    /// If the `genesis_hash` is specified then it will supercede the one inferred from
    /// `keyring.network` in case of conflict.
    pub fn create_with_params(mut params: CreateParams<K>) -> Result<Self, KeyRingError<K>> {
        let network = params.keyring.network;
        let genesis_inferred = bitcoin::constants::genesis_block(network).block_hash();

        let (chain, chain_changeset) =
            LocalChain::from_genesis_hash(params.genesis_hash.unwrap_or(genesis_inferred));

        let mut index = KeychainTxOutIndex::new(params.lookahead, params.use_spk_cache);

        let descriptors = params.keyring.descriptors.clone();
        for (keychain, desc) in descriptors {
            let _inserted = index
                .insert_descriptor(keychain.clone(), desc.clone())
                .map_err(|e| match e {
                    InsertDescriptorError::DescriptorAlreadyAssigned { .. } => {
                        KeyRingError::DescAlreadyExists(Box::new(desc))
                    }
                    InsertDescriptorError::KeychainAlreadyAssigned { .. } => {
                        KeyRingError::KeychainAlreadyExists(keychain)
                    }
                })?;
            assert!(_inserted);
        }

        let tx_graph = IndexedTxGraph::new(index);

        let locked_outpoints = HashSet::new();

        let stage = ChangeSet {
            keyring: params.keyring.initial_changeset(),
            local_chain: chain_changeset,
            tx_graph: bdk_chain::tx_graph::ChangeSet::default(),
            indexer: bdk_chain::keychain_txout::ChangeSet::default(),
            locked_outpoints: locked_outpoints::ChangeSet::default(),
        };

        Ok(Self {
            keyring: params.keyring,
            chain,
            tx_graph,
            stage,
            locked_outpoints,
        })
    }

    /// Build [`Wallet`] by loading from persistence or [`ChangeSet`].
    ///
    /// Note that the descriptor secret keys are not persisted to the db.
    /// You can check the wallet's descriptors are what you expect with [`LoadParams::check_descs`].
    /// Similarly you can check the [`Network`] and `genesis_hash`.
    /// [`LoadParams::lookahead`] and [`LoadParams::use_spk_cache`] can be used to set those values
    /// for [`Wallet`].
    pub fn load() -> LoadParams<K> {
        LoadParams::new()
    }

    /// Construct a [`Wallet`] from a [`ChangeSet`]
    pub fn load_with_params(
        changeset: ChangeSet<K>,
        params: LoadParams<K>,
    ) -> Result<Option<Self>, LoadError<K>> {
        if changeset.is_empty() {
            return Ok(None);
        }

        let keyring =
            KeyRing::from_changeset(changeset.keyring, params.check_network, params.check_descs)
                .map_err(|err| LoadError::InvalidKeyRing(err))?
                .ok_or(LoadError::EmptyKeyring)?;

        let local_chain = LocalChain::from_changeset(changeset.local_chain)
            .map_err(|_| LoadError::MissingGenesis)?;

        if let Some(exp_genesis_hash) = params.check_genesis_hash {
            if exp_genesis_hash != local_chain.genesis_hash() {
                Err(LoadError::GenesisMismatch {
                    loaded: local_chain.genesis_hash(),
                    expected: exp_genesis_hash,
                })?
            }
        }

        let mut stage = ChangeSet::default();

        let tx_graph = make_indexed_graph(
            &mut stage,
            changeset.tx_graph,
            changeset.indexer,
            keyring.descriptors.clone(),
            params.lookahead,
            params.use_spk_cache,
        )
        .map_err(LoadError::InvalidKeyRing)?;

        let locked_outpoints = changeset.locked_outpoints.outpoints;
        let locked_outpoints = locked_outpoints
            .into_iter()
            .filter(|&(_op, is_locked)| is_locked)
            .map(|(op, _)| op)
            .collect();

        Ok(Some(Wallet {
            keyring,
            chain: local_chain,
            tx_graph,
            stage,
            locked_outpoints,
        }))
    }
}

// This impl block contains wallet information getters
impl<K> Wallet<K>
where
    K: Clone + Debug + Ord,
{
    /// Get a reference to the [`Wallet`]'s [`Network`].
    pub fn network(&self) -> Network {
        self.keyring.network()
    }

    /// Get the (`keychain, descriptor`) pairs owned by the [`Wallet`]
    pub fn keychains(&self) -> &BTreeMap<K, Descriptor<DescriptorPublicKey>> {
        self.keyring.list_keychains()
    }

    /// Get a reference to the inner [`TxGraph`].
    pub fn tx_graph(&self) -> &TxGraph<ConfirmationBlockTime> {
        self.tx_graph.graph()
    }

    /// Get a reference to the inner [`KeychainTxOutIndex`].
    pub fn spk_index(&self) -> &KeychainTxOutIndex<K> {
        &self.tx_graph.index
    }

    /// Get a reference to the inner [`LocalChain`].
    pub fn local_chain(&self) -> &LocalChain {
        &self.chain
    }

    /// Returns the latest checkpoint.
    pub fn latest_checkpoint(&self) -> CheckPoint {
        self.chain.tip()
    }

    /// Get all the checkpoints the wallet is currently storing indexed by height.
    pub fn checkpoints(&self) -> CheckPointIter {
        self.chain.iter_checkpoints()
    }

    // TODO PR #318: I think this one can be removed (users should be able to get the checksum for
    //               for their descriptors from the KeyRing), but I just want to make sure. If it
    //               stays, it should return an Option<String> in case the keychain provided doesn't
    //               exist.
    /// Return the checksum of the public descriptor associated to the `keychain`.
    pub fn descriptor_checksum(&self, keychain: K) -> String {
        self.keychains()
            .get(&keychain)
            .unwrap()
            .to_string()
            .split_once('#')
            .unwrap()
            .1
            .to_string()
    }
}

// This impl block contains address, scripts, and other related methods
impl<K> Wallet<K>
where
    K: Clone + Debug + Ord,
{
    /// Attempt to reveal the next address of the given `keychain`.
    ///
    /// This will increment the keychain's derivation index. If the keychain's descriptor doesn't
    /// contain a wildcard or every address is already revealed up to the maximum derivation
    /// index defined in [BIP32](https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki),
    /// then the last revealed address will be returned.
    ///
    /// **WARNING**: To avoid address reuse you must persist the changes resulting from one or more
    /// calls to this method before closing the wallet. For example:
    // TODO: Fix the following example:
    // ///
    // /// ```rust,no_run
    // /// # use bdk_wallet::{LoadParams, ChangeSet, KeychainKind};
    // /// use bdk_chain::rusqlite::Connection;
    // /// let mut conn = Connection::open_in_memory().expect("must open connection");
    // /// let mut wallet = LoadParams::new()
    // ///     .load_wallet(&mut conn)
    // ///     .expect("database is okay")
    // ///     .expect("database has data");
    // /// let next_address = wallet.reveal_next_address(KeychainKind::External);
    // /// wallet.persist(&mut conn).expect("write is okay");
    // ///
    // /// // Now it's safe to show the user their next address!
    // /// println!("Next address: {}", next_address.address);
    // /// # Ok::<(), anyhow::Error>(())
    // /// ```
    pub fn reveal_next_address(&mut self, keychain: K) -> Option<AddressInfo<K>> {
        let index = &mut self.tx_graph.index;
        let stage = &mut self.stage;

        let ((index, spk), index_changeset) = index.reveal_next_spk(keychain.clone())?;

        stage.merge(index_changeset.into());

        Some(AddressInfo {
            index,
            address: Address::from_script(spk.as_script(), self.keyring.network)
                .expect("must have address form"),
            keychain,
        })
    }

    /// Get the next unused address for the given `keychain`, i.e. the address with the lowest
    /// derivation index that hasn't been used in a transaction.
    ///
    /// This will attempt to reveal a new address if all previously revealed addresses have
    /// been used, in which case the returned address will be the same as calling
    /// [`Wallet::reveal_next_address`].
    ///
    /// **WARNING**: To avoid address reuse you must persist the changes resulting from one or more
    /// calls to this method before closing the wallet. See [`Wallet::reveal_next_address`].
    pub fn next_unused_address(&mut self, keychain: K) -> Option<AddressInfo<K>> {
        let index = &mut self.tx_graph.index;

        let ((index, spk), index_changeset) = index.next_unused_spk(keychain.clone())?;

        self.stage
            .merge(indexed_tx_graph::ChangeSet::from(index_changeset).into());

        Some(AddressInfo {
            index,
            address: Address::from_script(spk.as_script(), self.keyring.network)
                .expect("must have address form"),
            keychain,
        })
    }

    /// Peek an address of the given `keychain` at `index` without revealing it.
    ///
    /// For non-wildcard descriptors this returns the same address at every provided index.
    ///
    /// # Panics
    ///
    /// This panics when the caller requests for an address of derivation index greater than the
    /// [BIP32](https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki) max index.
    pub fn peek_address(&self, keychain: K, mut index: u32) -> Option<AddressInfo<K>> {
        let mut spk_iter = self.tx_graph.index.unbounded_spk_iter(keychain.clone())?;
        if !spk_iter.descriptor().has_wildcard() {
            index = 0;
        }
        let (index, spk) = spk_iter
            .nth(index as usize)
            .expect("derivation index is out of bounds");

        Some(AddressInfo {
            index,
            address: Address::from_script(&spk, self.network()).expect("must have address form"),
            keychain,
        })
    }

    // /// TODO PR #318: Finish this one, I didn't quite get it done and had to stop for the day.
    // /// Reveal addresses up to and including the target `index` and return an iterator
    // /// of newly revealed addresses.
    // ///
    // /// If the target `index` is unreachable, we make a best effort to reveal up to the last
    // /// possible index. If all addresses up to the given `index` are already revealed, then
    // /// no new addresses are returned.
    // ///
    // /// **WARNING**: To avoid address reuse you must persist the changes resulting from one or
    // /// more calls to this method before closing the wallet. See [`Wallet::reveal_next_address`].
    // pub fn reveal_addresses_to(
    //     &mut self,
    //     keychain: K,
    //     index: u32,
    // ) -> Option<impl Iterator<Item = AddressInfo<K>> + '_> {
    //     let (spks, index_changeset) = self
    //         .tx_graph
    //         .index
    //         .reveal_to_target(keychain.clone(), index)?;
    //
    //     self.stage.merge(index_changeset.into());
    //
    //     spks.into_iter().map(move |(index, spk)| AddressInfo {
    //         index,
    //         address: Address::from_script(&spk, self.network()).expect("must have address form"),
    //         keychain,
    //     })
    // }

    // TODO PR #318: Finish this one.
    // /// List addresses that are revealed but unused.
    // ///
    // /// Note: if the returned iterator is empty, you can reveal more addresses
    // /// by using [`reveal_next_address`](Self::reveal_next_address) or
    // /// [`reveal_addresses_to`](Self::reveal_addresses_to).
    // pub fn list_unused_addresses(
    //     &self,
    //     keychain: K,
    // ) -> impl DoubleEndedIterator<Item = AddressInfo<K>> + '_ {
    //     self.indexed_graph
    //         .index
    //         .unused_keychain_spks(keychain)
    //         .map(move |(index, spk)| AddressInfo {
    //             index,
    //             address: Address::from_script(spk.as_script(), self.network)
    //                 .expect("must have address form"),
    //             keychain,
    //         })
    // }

    // TODO PR #318: This is slightly buggy and should probably return an Option in case the
    //               keychain doesn't exist, or a different function signature entirely if needed.
    /// Marks an address used of the given `keychain` at `index`.
    ///
    /// Returns whether the given index was present and then removed from the unused set.
    pub fn mark_used(&mut self, keychain: K, index: u32) -> bool {
        self.tx_graph.index.mark_used(keychain, index)
    }

    // TODO PR #318: This is slightly buggy and should probably return an Option in case the
    //               keychain doesn't exist, or a different function signature entirely if needed.
    /// Undoes the effect of [`mark_used`] and returns whether the `index` was inserted
    /// back into the unused set.
    ///
    /// Since this is only a superficial marker, it will have no effect if the address at the
    /// given `index` was actually used, i.e. the wallet has previously indexed a tx output for
    /// the derived spk.
    pub fn unmark_used(&mut self, keychain: K, index: u32) -> bool {
        self.tx_graph.index.unmark_used(keychain, index)
    }

    // TODO PR #318: This is buggy in the sense that a user would not be able to know whether the
    //               method returned None because the keychain was not in the keyring or whether it
    //               was because not addresses were revealed on that keychain.
    /// The derivation index of this wallet for a given keychain. It will return `None` if it has
    /// not derived any addresses. Otherwise, it will return the index of the highest address it has
    /// derived.
    pub fn derivation_index(&self, keychain: K) -> Option<u32> {
        self.spk_index().last_revealed_index(keychain)
    }

    /// The index of the next address you would get if you were to ask the wallet for a new address
    /// on keychain `K`.
    pub fn next_derivation_index(&self, keychain: K) -> Option<u32> {
        Some(self.tx_graph.index.next_index(keychain)?.0)
    }

    /// Return whether a `script` is part of this wallet (on any of its keychains).
    pub fn is_mine(&self, script: ScriptBuf) -> bool {
        self.tx_graph.index.index_of_spk(script).is_some()
    }

    /// Finds how the wallet derived the script pubkey `spk`.
    ///
    /// Will only return `Some(_)` if the wallet has given out the spk.
    pub fn derivation_of_spk(&self, spk: ScriptBuf) -> Option<(K, u32)> {
        self.tx_graph.index.index_of_spk(spk).cloned()
    }

    /// Get unbounded script pubkey iterators for all keychains.
    ///
    /// This is intended to be used when doing a full scan of your addresses (e.g., after
    /// restoring from seed words). You pass the `BTreeMap` of iterators to a blockchain data
    /// source (e.g., electrum server) which will go through each address until it reaches a
    /// *stop gap*.
    /// Note carefully that iterators go over **all** script pubkeys on the keychains (not what
    /// script pubkeys the wallet is storing internally).
    pub fn all_unbounded_spk_iters(
        &self,
    ) -> BTreeMap<K, impl Iterator<Item = Indexed<ScriptBuf>> + Clone> {
        self.tx_graph.index.all_unbounded_spk_iters()
    }

    /// Get an unbounded script pubkey iterator for the given keychain `K`. Returns None if the
    /// keychain doesn't exist.
    ///
    /// See [`all_unbounded_spk_iters`] for more documentation.
    ///
    /// [`all_unbounded_spk_iters`]: Self::all_unbounded_spk_iters
    pub fn unbounded_spk_iter(
        &self,
        keychain: K,
    ) -> Option<impl Iterator<Item = Indexed<ScriptBuf>> + Clone> {
        self.tx_graph.index.unbounded_spk_iter(keychain)
    }

    /// Returns the utxo owned by this wallet corresponding to `outpoint` if it exists in the
    /// wallet's database.
    pub fn get_utxo(&self, op: OutPoint) -> Option<LocalOutput<K>> {
        let ((keychain, index), _) = self.tx_graph.index.txout(op)?;
        self.tx_graph
            .graph()
            .filter_chain_unspents(
                &self.chain,
                self.chain.tip().block_id(),
                CanonicalizationParams::default(),
                core::iter::once(((), op)),
            )
            .map(|(_, full_txo)| new_local_utxo(keychain.clone(), index, full_txo))
            .next()
    }
}

// This impl block contains methods related to locked outpoints
impl<K> Wallet<K>
where
    K: Ord + Clone + Debug,
{
    /// List the locked outpoints.
    pub fn list_locked_outpoints(&self) -> impl Iterator<Item = OutPoint> + '_ {
        self.locked_outpoints.iter().copied()
    }

    /// List unspent outpoints that are currently locked.
    pub fn list_locked_unspent(&self) -> impl Iterator<Item = OutPoint> + '_ {
        self.list_unspent()
            .filter(|output| self.is_outpoint_locked(output.outpoint))
            .map(|output| output.outpoint)
    }

    /// Whether the `outpoint` is locked. See [`Wallet::lock_outpoint`] for more.
    pub fn is_outpoint_locked(&self, outpoint: OutPoint) -> bool {
        self.locked_outpoints.contains(&outpoint)
    }

    /// Lock a wallet output identified by the given `outpoint`.
    ///
    /// A locked UTXO will not be selected as an input to fund a transaction. This is useful
    /// for excluding or reserving candidate inputs during transaction creation.
    ///
    /// **You must persist the staged change for the lock status to be persistent**. To unlock a
    /// previously locked outpoint, see [`Wallet::unlock_outpoint`].
    pub fn lock_outpoint(&mut self, outpoint: OutPoint) {
        if self.locked_outpoints.insert(outpoint) {
            let changeset = locked_outpoints::ChangeSet {
                outpoints: [(outpoint, true)].into(),
            };
            self.stage.merge(changeset.into());
        }
    }

    /// Unlock the wallet output of the specified `outpoint`.
    ///
    /// **You must persist the staged change for the lock status to be persistent**.
    pub fn unlock_outpoint(&mut self, outpoint: OutPoint) {
        if self.locked_outpoints.remove(&outpoint) {
            let changeset = locked_outpoints::ChangeSet {
                outpoints: [(outpoint, false)].into(),
            };
            self.stage.merge(changeset.into());
        }
    }
}

// This impl block contains methods related to transactions and transaction building.
impl<K> Wallet<K>
where
    K: Clone + Debug + Ord,
{
    /// Iterate over relevant and canonical transactions in the wallet.
    ///
    /// A transaction is relevant if it spends from at least one a tracked output or spends to at
    /// least one tracked script pubkey. A transaction is canonical when it is confirmed in the
    /// best chain or does not conflict with a transaction confirmed in the best chain.
    pub fn transactions<'a>(
        &'a self,
    ) -> impl Iterator<Item = CanonicalTx<'a, Arc<Transaction>, ConfirmationBlockTime>> + 'a {
        let tx_graph = self.tx_graph.graph();
        let index = &self.tx_graph.index;
        tx_graph
            .list_canonical_txs(
                &self.chain,
                self.chain.tip().block_id(),
                CanonicalizationParams::default(),
            )
            .filter(|c_tx| index.is_tx_relevant(&c_tx.tx_node.tx))
    }

    /// Array of relevant and canonical transactions in the wallet sorted with a comparator
    /// function.
    ///
    /// This is a helper method equivalent to collecting the result of [`Wallet::transactions`]
    /// into a [`Vec`] and then sorting it.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdk_wallet::{KeychainKind, LoadParams, Wallet, WalletTx};
    /// # let mut wallet:Wallet<KeychainKind> = todo!();
    /// // Transactions by chain position: first unconfirmed, then descending by confirmed height.
    /// let sorted_txs: Vec<WalletTx> =
    ///     wallet.transactions_sort_by(|tx1, tx2| tx2.chain_position.cmp(&tx1.chain_position));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn transactions_sort_by<F>(&self, compare: F) -> Vec<WalletTx<'_>>
    where
        F: FnMut(&WalletTx, &WalletTx) -> Ordering,
    {
        let mut txs: Vec<WalletTx> = self.transactions().collect();
        txs.sort_unstable_by(compare);
        txs
    }

    /// Get a single transaction from the wallet as a [`WalletTx`] (if the transaction exists).
    ///
    /// `WalletTx` contains the full transaction alongside meta-data such as:
    /// * Blocks that the transaction is [`Anchor`]ed in. These may or may not be blocks that exist
    ///   in the best chain.
    /// * The [`ChainPosition`] of the transaction in the best chain - whether the transaction is
    ///   confirmed or unconfirmed. If the transaction is confirmed, the anchor which proves the
    ///   confirmation is provided. If the transaction is unconfirmed, the unix timestamp of when
    ///   the transaction was last seen in the mempool is provided.
    ///
    /// ```rust, no_run
    /// use bdk_chain::Anchor;
    /// use bdk_wallet::{chain::ChainPosition, KeychainKind, Wallet};
    /// # let wallet: Wallet<KeychainKind> = todo!();
    /// # let my_txid: bitcoin::Txid = todo!();
    ///
    /// let wallet_tx = wallet.get_tx(my_txid).expect("panic if tx does not exist");
    ///
    /// // get reference to full transaction
    /// println!("my tx: {:#?}", wallet_tx.tx_node.tx);
    ///
    /// // list all transaction anchors
    /// for anchor in wallet_tx.tx_node.anchors {
    ///     println!(
    ///         "tx is anchored by block of hash {}",
    ///         anchor.anchor_block().hash
    ///     );
    /// }
    ///
    /// // get confirmation status of transaction
    /// match wallet_tx.chain_position {
    ///     ChainPosition::Confirmed {
    ///         anchor,
    ///         transitively: None,
    ///     } => println!(
    ///         "tx is confirmed at height {}, we know this since {}:{} is in the best chain",
    ///         anchor.block_id.height, anchor.block_id.height, anchor.block_id.hash,
    ///     ),
    ///     ChainPosition::Confirmed {
    ///         anchor,
    ///         transitively: Some(_),
    ///     } => println!(
    ///         "tx is an ancestor of a tx anchored in {}:{}",
    ///         anchor.block_id.height, anchor.block_id.hash,
    ///     ),
    ///     ChainPosition::Unconfirmed { first_seen, last_seen } => println!(
    ///         "tx is first seen at {:?}, last seen at {:?}, it is unconfirmed as it is not anchored in the best chain",
    ///         first_seen, last_seen
    ///     ),
    /// }
    /// ```
    ///
    /// [`Anchor`]: bdk_chain::Anchor
    pub fn get_tx(&self, txid: Txid) -> Option<WalletTx<'_>> {
        let graph = self.tx_graph.graph();
        graph
            .list_canonical_txs(
                &self.chain,
                self.chain.tip().block_id(),
                CanonicalizationParams::default(),
            )
            .find(|tx| tx.tx_node.txid == txid)
    }

    /// Return the list of unspent outputs of this wallet
    pub fn list_unspent(&self) -> impl Iterator<Item = LocalOutput<K>> + '_ {
        self.tx_graph
            .graph()
            .filter_chain_unspents(
                &self.chain,
                self.chain.tip().block_id(),
                CanonicalizationParams::default(),
                self.tx_graph.index.outpoints().iter().cloned(),
            )
            .map(|((k, i), full_txo)| new_local_utxo(k, i, full_txo))
    }

    /// Compute the `tx`'s sent and received [`Amount`]s.
    ///
    /// This method returns a tuple `(sent, received)`. Sent is the sum of the txin amounts
    /// that spend from previous txouts tracked by this wallet. Received is the summation
    /// of this tx's outputs that send to script pubkeys tracked by this wallet.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// # use bitcoin::Txid;
    /// # use bdk_wallet::{KeychainKind, Wallet};
    /// # let mut wallet: Wallet<KeychainKind> = todo!();
    /// # let txid:Txid = todo!();
    /// let tx = wallet.get_tx(txid).expect("tx exists").tx_node.tx;
    /// let (sent, received) = wallet.sent_and_received(&tx);
    /// ```
    ///
    /// ```rust, no_run
    /// # use bitcoin::Psbt;
    /// # use bdk_wallet::{KeychainKind, Wallet};
    /// # let mut wallet: Wallet<KeychainKind> = todo!();
    /// # let mut psbt: Psbt = todo!();
    /// let tx = &psbt.clone().extract_tx().expect("tx");
    /// let (sent, received) = wallet.sent_and_received(tx);
    /// ```
    pub fn sent_and_received(&self, tx: &Transaction) -> (Amount, Amount) {
        self.tx_graph.index.sent_and_received(tx, ..)
    }

    /// Calculates the fee of a given transaction. Returns [`Amount::ZERO`] if `tx` is a coinbase
    /// transaction.
    ///
    /// To calculate the fee for a [`Transaction`] with inputs not owned by this wallet you must
    /// manually insert the TxOut(s) into the tx graph using the [`insert_txout`] function.
    ///
    /// Note `tx` does not have to be in the graph for this to work.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// # use bitcoin::Txid;
    /// # use bdk_wallet::{KeychainKind, Wallet};
    /// # let mut wallet: Wallet<KeychainKind> = todo!();
    /// # let txid:Txid = todo!();
    /// let tx = wallet.get_tx(txid).expect("transaction").tx_node.tx;
    /// let fee = wallet.calculate_fee(&tx).expect("fee");
    /// ```
    ///
    /// ```rust, no_run
    /// # use bitcoin::Psbt;
    /// # use bdk_wallet::{KeychainKind, Wallet};
    /// # let mut wallet: Wallet<KeychainKind> = todo!();
    /// # let mut psbt: Psbt = todo!();
    /// let tx = &psbt.clone().extract_tx().expect("tx");
    /// let fee = wallet.calculate_fee(tx).expect("fee");
    /// ```
    /// [`insert_txout`]: Self::insert_txout
    pub fn calculate_fee(&self, tx: &Transaction) -> Result<Amount, CalculateFeeError> {
        self.tx_graph.graph().calculate_fee(tx)
    }

    /// Calculate the [`FeeRate`] for a given transaction.
    ///
    /// To calculate the fee rate for a [`Transaction`] with inputs not owned by this wallet you
    /// must manually insert the TxOut(s) into the tx graph using the [`insert_txout`] function.
    ///
    /// Note `tx` does not have to be in the graph for this to work.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// # use bitcoin::Txid;
    /// # use bdk_wallet::{KeychainKind, Wallet};
    /// # let mut wallet: Wallet<KeychainKind> = todo!();
    /// # let txid:Txid = todo!();
    /// let tx = wallet.get_tx(txid).expect("transaction").tx_node.tx;
    /// let fee_rate = wallet.calculate_fee_rate(&tx).expect("fee rate");
    /// ```
    ///
    /// ```rust, no_run
    /// # use bitcoin::Psbt;
    /// # use bdk_wallet::{KeychainKind, Wallet};
    /// # let mut wallet: Wallet<KeychainKind> = todo!();
    /// # let mut psbt: Psbt = todo!();
    /// let tx = &psbt.clone().extract_tx().expect("tx");
    /// let fee_rate = wallet.calculate_fee_rate(tx).expect("fee rate");
    /// ```
    /// [`insert_txout`]: Self::insert_txout
    pub fn calculate_fee_rate(&self, tx: &Transaction) -> Result<FeeRate, CalculateFeeError> {
        self.calculate_fee(tx).map(|fee| fee / tx.weight())
    }

    /// Get the [`TxDetails`] of a wallet transaction.
    ///
    /// If the transaction with txid [`Txid`] cannot be found in the wallet's transactions, `None`
    /// is returned.
    pub fn tx_details(&self, txid: Txid) -> Option<TxDetails> {
        let tx: WalletTx = self.transactions().find(|c| c.tx_node.txid == txid)?;

        let (sent, received) = self.sent_and_received(&tx.tx_node.tx);
        let fee: Option<Amount> = self.calculate_fee(&tx.tx_node.tx).ok();
        let fee_rate: Option<FeeRate> = self.calculate_fee_rate(&tx.tx_node.tx).ok();
        let balance_delta: SignedAmount = self.tx_graph.index.net_value(&tx.tx_node.tx, ..);
        let chain_position = tx.chain_position;

        let tx_details: TxDetails = TxDetails {
            txid,
            received,
            sent,
            fee,
            fee_rate,
            balance_delta,
            chain_position,
            tx: tx.tx_node.tx,
        };

        Some(tx_details)
    }

    /// List all relevant outputs (includes both spent and unspent, confirmed and unconfirmed).
    ///
    /// To list only unspent outputs (UTXOs), use [`Wallet::list_unspent`] instead.
    pub fn list_output(&self) -> impl Iterator<Item = LocalOutput<K>> + '_ {
        self.tx_graph
            .graph()
            .filter_chain_txouts(
                &self.chain,
                self.chain.tip().block_id(),
                CanonicalizationParams::default(),
                self.tx_graph.index.outpoints().iter().cloned(),
            )
            .map(|((k, i), full_txo)| new_local_utxo(k, i, full_txo))
    }

    /// Informs the wallet that you no longer intend to broadcast a tx that was built from it.
    ///
    /// This frees up the change address used when creating the tx for use in future
    /// transactions.
    // TODO: Make this free up reserved utxos when that's implemented
    pub fn cancel_tx(&mut self, tx: &Transaction) {
        let txout_index = &mut self.tx_graph.index;
        for txout in &tx.output {
            if let Some((keychain, index)) = txout_index.index_of_spk(txout.script_pubkey.clone()) {
                // NOTE: unmark_used will **not** make something unused if it has actually been used
                // by a tx in the tracker. It only removes the superficial marking.
                txout_index.unmark_used(keychain.clone(), *index);
            }
        }
    }

    /// Inserts a [`TxOut`] at [`OutPoint`] into the wallet's transaction graph.
    ///
    /// This is used for providing a previous output's value so that we can use [`calculate_fee`]
    /// or [`calculate_fee_rate`] on a given transaction. Outputs inserted with this method will
    /// not be returned in [`list_unspent`] or [`list_output`].
    ///
    /// **WARNINGS:** This should only be used to add `TxOut`s that the wallet does not own. Only
    /// insert `TxOut`s that you trust the values for!
    ///
    /// You must persist the changes resulting from one or more calls to this method if you need
    /// the inserted `TxOut` data to be reloaded after closing the wallet.
    /// See [`Wallet::reveal_next_address`].
    ///
    /// [`calculate_fee`]: Self::calculate_fee
    /// [`calculate_fee_rate`]: Self::calculate_fee_rate
    /// [`list_unspent`]: Self::list_unspent
    /// [`list_output`]: Self::list_output
    pub fn insert_txout(&mut self, outpoint: OutPoint, txout: TxOut) {
        let additions = self.tx_graph.insert_txout(outpoint, txout);
        self.stage.merge(additions.into());
    }

    // TODO PR #318: Bring this one back.
    // /// Get the corresponding PSBT Input for a [`LocalOutput`].
    // pub fn get_psbt_input(
    //     &self,
    //     utxo: LocalOutput,
    //     sighash_type: Option<psbt::PsbtSighashType>,
    //     only_witness_utxo: bool,
    // ) -> Result<psbt::Input, CreateTxError> {
    //     // Try to find the prev_script in our db to figure out if this is internal or external,
    //     // and the derivation index.
    //     let &(keychain, child) = self
    //         .indexed_graph
    //         .index
    //         .index_of_spk(utxo.txout.script_pubkey)
    //         .ok_or(CreateTxError::UnknownUtxo)?;
    //
    //     let mut psbt_input = psbt::Input {
    //         sighash_type,
    //         ..psbt::Input::default()
    //     };
    //
    //     let desc = self.public_descriptor(keychain);
    //     let derived_descriptor = desc
    //         .at_derivation_index(child)
    //         .expect("child can't be hardened");
    //
    //     psbt_input
    //         .update_with_descriptor_unchecked(&derived_descriptor)
    //         .map_err(MiniscriptPsbtError::Conversion)?;
    //
    //     let prev_output = utxo.outpoint;
    //     if let Some(prev_tx) = self.indexed_graph.graph().get_tx(prev_output.txid) {
    //         // We want to check that the prevout actually exists in the transaction before
    //         // continuing.
    //         let prevout = prev_tx.output.get(prev_output.vout as usize).ok_or(
    //             MiniscriptPsbtError::UtxoUpdate(miniscript::psbt::UtxoUpdateError::UtxoCheck),
    //         )?;
    //         if desc.is_witness() || desc.is_taproot() {
    //             psbt_input.witness_utxo = Some(prevout.clone());
    //         }
    //         if !desc.is_taproot() && (!desc.is_witness() || !only_witness_utxo) {
    //             psbt_input.non_witness_utxo = Some(prev_tx.as_ref().clone());
    //         }
    //     }
    //     Ok(psbt_input)
    // }
}

// This impl block contains balance methods and related helper functions
impl<K> Wallet<K>
where
    K: Ord + Clone + Debug,
{
    // TODO PR #318: For now, all balances are "untrusted". Fix this (but might not be a fix that
    //               should arrive in #318).
    /// Return the balance, separated into available, trusted-pending, untrusted-pending, and
    /// immature values.
    pub fn balance(&self) -> Balance {
        self.tx_graph.graph().balance(
            &self.chain,
            self.chain.tip().block_id(),
            CanonicalizationParams::default(),
            self.tx_graph.index.outpoints().iter().cloned(),
            |_, _| false,
        )
    }

    // TODO PR #318: For now, all balances are "untrusted". Fix this (but might not be a fix that
    //               should arrive in #318).
    /// Return the balance for a given keychain. This balance is separated into available,
    /// trusted-pending, untrusted-pending, and immature values.
    pub fn balance_keychain(&self, keychain: K) -> Balance {
        self.tx_graph.graph().balance(
            &self.chain,
            self.chain.tip().block_id(),
            CanonicalizationParams::default(),
            self.tx_graph.index.keychain_outpoints(keychain),
            |_, _| false,
        )
    }
}

// This impl block contains all methods interacting with `Wallet::stage`.
impl<K> Wallet<K>
where
    K: Ord + Clone + Debug,
{
    fn stage(&mut self, changeset: impl Into<ChangeSet<K>>) {
        self.stage.merge(changeset.into());
    }

    /// Get a reference of the staged [`ChangeSet`] that is yet to be committed (if any).
    pub fn staged(&self) -> Option<&ChangeSet<K>> {
        if self.stage.is_empty() {
            None
        } else {
            Some(&self.stage)
        }
    }

    /// Get a mutable reference of the staged [`ChangeSet`] that is yet to be committed (if any).
    pub fn staged_mut(&mut self) -> Option<&mut ChangeSet<K>> {
        if self.stage.is_empty() {
            None
        } else {
            Some(&mut self.stage)
        }
    }

    /// Take the staged [`ChangeSet`] to be persisted now (if any).
    pub fn take_staged(&mut self) -> Option<ChangeSet<K>> {
        self.stage.take()
    }
}

// This impl block contains methods related to performing spk-based syncing.
/// Methods to construct sync/full-scan requests for spk-based chain sources.
impl<K> Wallet<K>
where
    K: Ord + Clone + Debug,
{
    /// Create a partial [`SyncRequest`] for all revealed spks.
    ///
    /// This is the first step while doing a spk-based wallet partial sync, the returned
    /// [`SyncRequest`] collects all revealed script pubkeys from the wallet keychain needed to
    /// start a blockchain sync with a spk based blockchain client.
    ///
    /// The time of the sync is the current system time and is used to record the last seen (or
    /// evicted) timestamps of mempool transactions. Note that the timestamps may only increase
    /// to be counted by the tx graph. To supply your own start time see
    /// [`start_sync_with_revealed_spks_at`].
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    #[cfg(feature = "std")]
    pub fn start_sync_with_revealed_spks(&self) -> SyncRequestBuilder<(K, u32)> {
        SyncRequest::builder()
            .chain_tip(self.chain.tip())
            .revealed_spks_from_indexer(&self.tx_graph.index, ..)
            .expected_spk_txids(self.tx_graph.list_expected_spk_txids(
                &self.chain,
                self.chain.tip().block_id(),
                ..,
            ))
    }

    /// Create a partial [`SyncRequest`] for all revealed spks at `start_time`.
    pub fn start_sync_with_revealed_spks_at(
        &self,
        start_time: u64,
    ) -> SyncRequestBuilder<(K, u32)> {
        SyncRequest::builder_at(start_time)
            .chain_tip(self.chain.tip())
            .revealed_spks_from_indexer(&self.tx_graph.index, ..)
            .expected_spk_txids(self.tx_graph.list_expected_spk_txids(
                &self.chain,
                self.chain.tip().block_id(),
                ..,
            ))
    }

    /// Create a [`FullScanRequest`] at `start_time`.
    ///
    /// This is the first step in spk-based wallet full scan, the returned [`FullScanRequest`]
    /// collects iterators for the wallet's keychain spks needed for a full scan.
    ///
    /// Full scan is generally used when importing or restoring an already used wallet when used
    /// spks are not known.
    ///
    /// The time of the scan is the current system time and is used to record the last seen (or
    /// evicted) timestamps of the mempool transactions. Note that the timestamps may only
    /// increase to be counted by the tx graph. To use a custom time see [`start_full_scan_at`].
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    #[cfg(feature = "std")]
    pub fn start_full_scan(&self) -> FullScanRequestBuilder<K> {
        FullScanRequest::builder()
            .chain_tip(self.chain.tip())
            .spks_from_indexer(&self.tx_graph.index)
    }

    /// Create a [`FullScanRequest`] builder at the `start_time`.
    pub fn start_full_scan_at(&self, start_time: u64) -> FullScanRequestBuilder<K> {
        FullScanRequest::builder_at(start_time)
            .chain_tip(self.chain.tip())
            .spks_from_indexer(&self.tx_graph.index)
    }

    /// Apply the update.
    pub fn apply_update(&mut self, update: impl Into<Update<K>>) -> Result<(), CannotConnectError> {
        let Update {
            last_active_indices,
            tx_update,
            chain,
        } = update.into();

        let mut changeset = ChangeSet::default();

        if let Some(tip) = chain {
            changeset.merge(self.chain.apply_update(tip)?.into());
        }

        changeset.merge(
            self.tx_graph
                .index
                .reveal_to_target_multi(&last_active_indices)
                .into(),
        );

        changeset.merge(self.tx_graph.apply_update(tx_update).into());

        self.stage(changeset);

        Ok(())
    }

    /// Applies an update to the wallet, stages the changes, and returns events.
    ///
    /// Usually you create an `update` by interacting with some blockchain data source and inserting
    /// transactions related to your wallet into it. Staged changes are NOT persisted.
    ///
    /// After applying updates you should process the events in your app before persisting the
    /// staged wallet changes. For an example of how to persist staged wallet changes see
    /// [`Wallet::reveal_next_address`].
    // TODO PR #318: Fix this.
    // /// ```rust,no_run
    // /// # use bitcoin::*;
    // /// # use bdk_wallet::*;
    // /// use bdk_wallet::WalletEvent;
    // /// # let wallet_update = Update::default();
    // /// # let mut wallet = doctest_wallet!();
    // /// let events = wallet.apply_update_events(wallet_update)?;
    // /// // Handle wallet relevant events from this update.
    // /// events.iter().for_each(|event| {
    // ///     match event {
    // ///         // The chain tip changed.
    // ///         WalletEvent::ChainTipChanged { old_tip, new_tip } => {
    // ///             todo!() // handle event
    // ///         }
    // ///         // An unconfirmed tx is now confirmed in a block.
    // ///         WalletEvent::TxConfirmed {
    // ///             txid,
    // ///             tx,
    // ///             block_time,
    // ///             old_block_time: None,
    // ///         } => {
    // ///             todo!() // handle event
    // ///         }
    // ///         // A confirmed tx is now confirmed in a new block (reorg).
    // ///         WalletEvent::TxConfirmed {
    // ///             txid,
    // ///             tx,
    // ///             block_time,
    // ///             old_block_time: Some(old_block_time),
    // ///         } => {
    // ///             todo!() // handle event
    // ///         }
    // ///         // A new unconfirmed tx was seen in the mempool.
    // ///         WalletEvent::TxUnconfirmed {
    // ///             txid,
    // ///             tx,
    // ///             old_block_time: None,
    // ///         } => {
    // ///             todo!() // handle event
    // ///         }
    // ///         // A previously confirmed tx in now unconfirmed in the mempool (reorg).
    // ///         WalletEvent::TxUnconfirmed {
    // ///             txid,
    // ///             tx,
    // ///             old_block_time: Some(old_block_time),
    // ///         } => {
    // ///             todo!() // handle event
    // ///         }
    // ///         // An unconfirmed tx was replaced in the mempool (RBF or double spent input).
    // ///         WalletEvent::TxReplaced {
    // ///             txid,
    // ///             tx,
    // ///             conflicts,
    // ///         } => {
    // ///             todo!() // handle event
    // ///         }
    // ///         // An unconfirmed tx was dropped from the mempool (fee too low).
    // ///         WalletEvent::TxDropped { txid, tx } => {
    // ///             todo!() // handle event
    // ///         }
    // ///         _ => {
    // ///             // unexpected event, do nothing
    // ///         }
    // ///     }
    // ///     // take staged wallet changes
    // ///     let staged = wallet.take_staged();
    // ///     // persist staged changes
    // /// });
    // /// # Ok::<(), anyhow::Error>(())
    // /// ```
    /// [`TxBuilder`]: crate::TxBuilder
    pub fn apply_update_events(
        &mut self,
        update: impl Into<Update<K>>,
    ) -> Result<Vec<WalletEvent>, CannotConnectError> {
        // snapshot of chain tip and transactions before update
        let chain_tip1 = self.chain.tip().block_id();
        let wallet_txs1 = self
            .transactions()
            .map(|wtx| {
                (
                    wtx.tx_node.txid,
                    (wtx.tx_node.tx.clone(), wtx.chain_position),
                )
            })
            .collect::<BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>>();

        // apply update
        self.apply_update(update)?;

        // chain tip and transactions after update
        let chain_tip2 = self.chain.tip().block_id();
        let wallet_txs2 = self
            .transactions()
            .map(|wtx| {
                (
                    wtx.tx_node.txid,
                    (wtx.tx_node.tx.clone(), wtx.chain_position),
                )
            })
            .collect::<BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>>();

        Ok(wallet_events(
            self,
            chain_tip1,
            chain_tip2,
            wallet_txs1,
            wallet_txs2,
        ))
    }
}

// This impl block contains methods related to performing block by block syncing.
/// Methods for performing block by block syncing
impl<K> Wallet<K>
where
    K: Ord + Clone + Debug,
{
    /// Introduces a `block` of `height` to the wallet, and tries to connect it to the
    /// `prev_blockhash` of the block's header.
    ///
    /// This is a convenience method that is equivalent to calling [`apply_block_connected_to`]
    /// with `prev_blockhash` and `height-1` as the `connected_to` parameter.
    ///
    /// [`apply_block_connected_to`]: Self::apply_block_connected_to
    pub fn apply_block(&mut self, block: &Block, height: u32) -> Result<(), CannotConnectError> {
        let connected_to = match height.checked_sub(1) {
            Some(prev_height) => BlockId {
                height: prev_height,
                hash: block.header.prev_blockhash,
            },
            None => BlockId {
                height,
                hash: block.block_hash(),
            },
        };
        self.apply_block_connected_to(block, height, connected_to)
            .map_err(|err| match err {
                ApplyHeaderError::InconsistentBlocks => {
                    unreachable!("connected_to is derived from the block so must be consistent")
                }
                ApplyHeaderError::CannotConnect(err) => err,
            })
    }

    /// Add transactions from `block` at `height` to [`Wallet`] and connects the `block` to the
    /// internal chain.
    ///
    /// The `connected_to` parameter specifies how this `block` connects to the internal
    /// [`LocalChain`]. Relevant transactions are filtered from the `block` and inserted into
    /// the internal [`TxGraph`].
    ///
    /// **WARNING**: The wallet must be persisted after a call to this method if you need the
    /// inserted block data to be available after a reload.
    pub fn apply_block_connected_to(
        &mut self,
        block: &Block,
        height: u32,
        connected_to: BlockId,
    ) -> Result<(), ApplyHeaderError> {
        let mut changeset = ChangeSet::default();
        changeset.merge(
            self.chain
                .apply_header_connected_to(&block.header, height, connected_to)?
                .into(),
        );
        changeset.merge(self.tx_graph.apply_block_relevant(block, height).into());
        self.stage.merge(changeset);
        Ok(())
    }

    /// Adds relevant unconfirmed transactions to the [`Wallet`]
    ///
    /// Irrelevant transactions are filtered out.
    ///
    /// This method takes in an iterator of `(transaction, last_seen)` where `last_seen` is the
    /// timestamp when the transaction was last seen in the mempool. In case of a conflicting
    /// unconfirmed transaction, the transaction with the later `last_seen` is prioritized.
    ///
    /// **WARNING**: You must persist the changes resulting from one or more calls to this method
    /// if you need the applied unconfirmed transactions to be reloaded after closing the wallet.
    /// See [`Wallet::reveal_next_address`].
    pub fn apply_unconfirmed_txs<T: Into<Arc<Transaction>>>(
        &mut self,
        unconfirmed_txs: impl IntoIterator<Item = (T, u64)>,
    ) {
        let changeset = self
            .tx_graph
            .batch_insert_relevant_unconfirmed(unconfirmed_txs);
        self.stage.merge(changeset.into())
    }

    /// Apply evictions of the given transaction IDs with their associated timestamps.
    ///
    /// This function is used to mark specific unconfirmed transactions as evicted from the mempool.
    /// Eviction means that these transactions are not considered canonical by default, and will
    /// no longer be part of the wallet's [`transactions`] set. This can happen for example when
    /// a transaction is dropped from the mempool due to low fees or conflicts with another
    /// transaction.
    ///
    /// Only transactions that are currently unconfirmed and canonical are considered for eviction.
    /// Transactions that are not relevant to the wallet are ignored. Note that an evicted
    /// transaction can become canonical again if it is later observed on-chain or seen in the
    /// mempool with a higher priority (e.g., due to a fee bump).
    ///
    /// ## Parameters
    ///
    /// `evicted_txs`: An iterator of `(Txid, u64)` tuples, where:
    /// - `Txid`: The transaction ID of the transaction to be evicted.
    /// - `u64`: The timestamp indicating when the transaction was evicted from the mempool. This
    ///   will usually correspond to the time of the latest chain sync. See docs for
    ///   [`start_sync_with_revealed_spks`].
    ///
    /// ## Notes
    ///
    /// - Not all blockchain backends support automatic mempool eviction handling - this method may
    ///   be used in such cases. It can also be used to negate the effect of
    ///   [`apply_unconfirmed_txs`] for a particular transaction without the need for an additional
    ///   sync.
    /// - The changes are staged in the wallet's internal state and must be persisted to ensure they
    ///   are retained across wallet restarts. Use [`Wallet::take_staged`] to retrieve the staged
    ///   changes and persist them to your database of choice.
    /// - Evicted transactions are removed from the wallet's canonical transaction set, but the data
    ///   remains in the wallet's internal transaction graph for historical purposes.
    /// - Ensure that the timestamps provided are accurate and monotonically increasing, as they
    ///   influence the wallet's canonicalization logic.
    ///
    /// [`transactions`]: Wallet::transactions
    /// [`apply_unconfirmed_txs`]: Wallet::apply_unconfirmed_txs
    /// [`start_sync_with_revealed_spks`]: Wallet::start_sync_with_revealed_spks
    pub fn apply_evicted_txs(&mut self, evicted_txs: impl IntoIterator<Item = (Txid, u64)>) {
        let chain = &self.chain;
        let canon_txids: BTreeSet<Txid> = self
            .tx_graph
            .graph()
            .list_canonical_txs(
                chain,
                chain.tip().block_id(),
                CanonicalizationParams::default(),
            )
            .map(|c_tx| c_tx.tx_node.txid)
            .collect();
        let changeset = self.tx_graph.batch_insert_relevant_evicted_at(
            evicted_txs
                .into_iter()
                .filter(|(txid, _)| canon_txids.contains(txid)),
        );
        self.stage.merge(changeset.into())
    }

    /// Introduces a `block` of `height` to the wallet, and tries to connect it to the
    /// `prev_blockhash` of the block's header.
    ///
    /// This is a convenience method that is equivalent to calling
    /// [`apply_block_connected_to_events`] with `prev_blockhash` and `height-1` as the
    /// `connected_to` parameter.
    ///
    /// See [`apply_update_events`] for more information on the returned [`WalletEvent`]s.
    ///
    /// [`apply_block_connected_to_events`]: Self::apply_block_connected_to_events
    /// [`apply_update_events`]: Self::apply_update_events
    pub fn apply_block_events(
        &mut self,
        block: &Block,
        height: u32,
    ) -> Result<Vec<WalletEvent>, CannotConnectError> {
        // snapshot of chain tip and transactions before update
        let chain_tip1 = self.chain.tip().block_id();
        let wallet_txs1 = self
            .transactions()
            .map(|wtx| {
                (
                    wtx.tx_node.txid,
                    (wtx.tx_node.tx.clone(), wtx.chain_position),
                )
            })
            .collect::<BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>>();

        self.apply_block(block, height)?;

        // chain tip and transactions after update
        let chain_tip2 = self.chain.tip().block_id();
        let wallet_txs2 = self
            .transactions()
            .map(|wtx| {
                (
                    wtx.tx_node.txid,
                    (wtx.tx_node.tx.clone(), wtx.chain_position),
                )
            })
            .collect::<BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>>();

        Ok(wallet_events(
            self,
            chain_tip1,
            chain_tip2,
            wallet_txs1,
            wallet_txs2,
        ))
    }

    /// Applies relevant transactions from `block` of `height` to the wallet, and connects the
    /// block to the internal chain.
    ///
    /// See [`apply_block_connected_to`] for more information.
    ///
    /// See [`apply_update_events`] for more information on the returned [`WalletEvent`]s.
    ///
    /// [`apply_block_connected_to`]: Self::apply_block_connected_to
    /// [`apply_update_events`]: Self::apply_update_events
    pub fn apply_block_connected_to_events(
        &mut self,
        block: &Block,
        height: u32,
        connected_to: BlockId,
    ) -> Result<Vec<WalletEvent>, ApplyHeaderError> {
        // snapshot of chain tip and transactions before update
        let chain_tip1 = self.chain.tip().block_id();
        let wallet_txs1 = self
            .transactions()
            .map(|wtx| {
                (
                    wtx.tx_node.txid,
                    (wtx.tx_node.tx.clone(), wtx.chain_position),
                )
            })
            .collect::<BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>>();

        self.apply_block_connected_to(block, height, connected_to)?;

        // chain tip and transactions after update
        let chain_tip2 = self.chain.tip().block_id();
        let wallet_txs2 = self
            .transactions()
            .map(|wtx| {
                (
                    wtx.tx_node.txid,
                    (wtx.tx_node.tx.clone(), wtx.chain_position),
                )
            })
            .collect::<BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>>();

        Ok(wallet_events(
            self,
            chain_tip1,
            chain_tip2,
            wallet_txs1,
            wallet_txs2,
        ))
    }
}

// impl Wallet {

// /// Finalize a PSBT, i.e., for each input determine if sufficient data is available to pass
// /// validation and construct the respective `scriptSig` or `scriptWitness`. Please refer to
// /// [BIP174](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki#Input_Finalizer),
// /// and [BIP371](https://github.com/bitcoin/bips/blob/master/bip-0371.mediawiki)
// /// for further information.
// ///
// /// Returns `true` if the PSBT could be finalized, and `false` otherwise.
// ///
// /// The [`SignOptions`] can be used to tweak the behavior of the finalizer.
// pub fn finalize_psbt(
//     &self,
//     psbt: &mut Psbt,
//     sign_options: SignOptions,
// ) -> Result<bool, SignerError> {
//     let tx = &psbt.unsigned_tx;
//     let chain_tip = self.chain.tip().block_id();
//     let prev_txids = tx
//         .input
//         .iter()
//         .map(|txin| txin.previous_output.txid)
//         .collect::<HashSet<Txid>>();
//     let confirmation_heights = self
//         .indexed_graph
//         .graph()
//         .list_canonical_txs(&self.chain, chain_tip, CanonicalizationParams::default())
//         .filter(|canon_tx| prev_txids.contains(&canon_tx.tx_node.txid))
//         // This is for a small performance gain. Although `.filter` filters out excess txs, it
//         // will still consume the internal `CanonicalIter` entirely. Having a `.take` here
//         // allows us to stop further unnecessary canonicalization.
//         .take(prev_txids.len())
//         .map(|canon_tx| {
//             let txid = canon_tx.tx_node.txid;
//             match canon_tx.chain_position {
//                 ChainPosition::Confirmed { anchor, .. } => (txid, anchor.block_id.height),
//                 ChainPosition::Unconfirmed { .. } => (txid, u32::MAX),
//             }
//         })
//         .collect::<HashMap<Txid, u32>>();
//
//     let mut finished = true;
//
//     for (n, input) in tx.input.iter().enumerate() {
//         let psbt_input = &psbt
//             .inputs
//             .get(n)
//             .ok_or(IndexOutOfBoundsError::new(n, psbt.inputs.len()))?;
//         if psbt_input.final_script_sig.is_some() || psbt_input.final_script_witness.is_some() {
//             continue;
//         }
//         let confirmation_height = confirmation_heights
//             .get(&input.previous_output.txid)
//             .copied();
//         let current_height = sign_options
//             .assume_height
//             .unwrap_or_else(|| self.chain.tip().height());
//
//         // - Try to derive the descriptor by looking at the txout. If it's in our database, we
//         //   know exactly which `keychain` to use, and which derivation index it is.
//         // - If that fails, try to derive it by looking at the psbt input: the complete logic is
//         //   in `src/descriptor/mod.rs`, but it will basically look at `bip32_derivation`,
//         //   `redeem_script` and `witness_script` to determine the right derivation.
//         // - If that also fails, it will try it on the internal descriptor, if present.
//         let desc = psbt
//             .get_utxo_for(n)
//             .and_then(|txout| self.get_descriptor_for_txout(&txout))
//             .or_else(|| {
//                 self.indexed_graph.index.keychains().find_map(|(_, desc)| {
//                     desc.derive_from_psbt_input(psbt_input, psbt.get_utxo_for(n), &self.secp)
//                 })
//             });
//
//         match desc {
//             Some(desc) => {
//                 let mut tmp_input = bitcoin::TxIn::default();
//                 match desc.satisfy(
//                     &mut tmp_input,
//                     (
//                         PsbtInputSatisfier::new(psbt, n),
//                         After::new(Some(current_height), false),
//                         Older::new(Some(current_height), confirmation_height, false),
//                     ),
//                 ) {
//                     Ok(_) => {
//                         let length = psbt.inputs.len();
//                         // Set the UTXO fields, final script_sig and witness
//                         // and clear everything else.
//                         let psbt_input = psbt
//                             .inputs
//                             .get_mut(n)
//                             .ok_or(IndexOutOfBoundsError::new(n, length))?;
//                         let original = mem::take(psbt_input);
//                         psbt_input.non_witness_utxo = original.non_witness_utxo;
//                         psbt_input.witness_utxo = original.witness_utxo;
//                         if !tmp_input.script_sig.is_empty() {
//                             psbt_input.final_script_sig = Some(tmp_input.script_sig);
//                         }
//                         if !tmp_input.witness.is_empty() {
//                             psbt_input.final_script_witness = Some(tmp_input.witness);
//                         }
//                     }
//                     Err(_) => finished = false,
//                 }
//             }
//             None => finished = false,
//         }
//     }
//
//     // Clear derivation paths from outputs.
//     if finished {
//         for output in &mut psbt.outputs {
//             output.bip32_derivation.clear();
//             output.tap_key_origins.clear();
//         }
//     }
//
//     Ok(finished)
// }

//     fn get_descriptor_for_txout(&self, txout: &TxOut) -> Option<DerivedDescriptor> {
//         let &(keychain, child) = self
//             .indexed_graph
//             .index
//             .index_of_spk(txout.script_pubkey.clone())?;
//         let descriptor = self.public_descriptor(keychain);
//         descriptor.at_derivation_index(child).ok()
//     }

//     /// Given the options returns the list of utxos that must be used to form the
//     /// transaction and any further that may be used if needed.
//     fn filter_utxos(&self, params: &TxParams, current_height: u32) -> Vec<WeightedUtxo> {
//         if params.manually_selected_only {
//             vec![]
//         // Only process optional UTxOs if manually_selected_only is false.
//         } else {
//             let manually_selected_outpoints = params
//                 .utxos
//                 .iter()
//                 .map(|wutxo| wutxo.utxo.outpoint())
//                 .collect::<HashSet<OutPoint>>();
//             self.indexed_graph
//                 .graph()
//                 // Get all unspent UTxOs from wallet.
//                 // NOTE: the UTxOs returned by the following method already belong to wallet as
// the                 // call chain uses get_tx_node infallibly.
//                 .filter_chain_unspents(
//                     &self.chain,
//                     self.chain.tip().block_id(),
//                     CanonicalizationParams::default(),
//                     self.indexed_graph.index.outpoints().iter().cloned(),
//                 )
//                 // Filter out locked outpoints.
//                 .filter(|(_, txo)| !self.is_outpoint_locked(txo.outpoint))
//                 // Only create LocalOutput if UTxO is mature.
//                 .filter_map(move |((k, i), full_txo)| {
//                     full_txo
//                         .is_mature(current_height)
//                         .then(|| new_local_utxo(k, i, full_txo))
//                 })
//                 // only process UTXOs not selected manually, they will be considered later in the
//                 // chain
//                 // NOTE: this avoid UTXOs in both required and optional list
//                 .filter(|may_spend| !manually_selected_outpoints.contains(&may_spend.outpoint))
//                 // only add to optional UTxOs those which satisfy the change policy if we reuse
//                 // change
//                 .filter(|local_output| {
//                     self.keychains().count() == 1
//                         || params.change_policy.is_satisfied_by(local_output)
//                 })
//                 // Only add to optional UTxOs those marked as spendable.
//                 .filter(|local_output| !params.unspendable.contains(&local_output.outpoint))
//                 // If bumping fees only add to optional UTxOs those confirmed.
//                 .filter(|local_output| {
//                     params.bumping_fee.is_none() || local_output.chain_position.is_confirmed()
//                 })
//                 .map(|utxo| WeightedUtxo {
//                     satisfaction_weight: self
//                         .public_descriptor(utxo.keychain)
//                         .max_weight_to_satisfy()
//                         .unwrap(),
//                     utxo: Utxo::Local(utxo),
//                 })
//                 .collect()
//         }
//     }

//     fn complete_transaction(
//         &self,
//         tx: Transaction,
//         selected: Vec<Utxo>,
//         params: TxParams,
//     ) -> Result<Psbt, CreateTxError> {
//         let mut psbt = Psbt::from_unsigned_tx(tx)?;

//         if params.add_global_xpubs {
//             let all_xpubs = self
//                 .keychains()
//                 .flat_map(|(_, desc)| desc.get_extended_keys())
//                 .collect::<Vec<_>>();

//             for xpub in all_xpubs {
//                 let origin = match xpub.origin {
//                     Some(origin) => origin,
//                     None if xpub.xkey.depth == 0 => {
//                         (xpub.root_fingerprint(&self.secp), vec![].into())
//                     }
//                     _ => return Err(CreateTxError::MissingKeyOrigin(xpub.xkey.to_string())),
//                 };

//                 psbt.xpub.insert(xpub.xkey, origin);
//             }
//         }

//         let mut lookup_output = selected
//             .into_iter()
//             .map(|utxo| (utxo.outpoint(), utxo))
//             .collect::<HashMap<_, _>>();

//         // Add metadata for the inputs.
//         for (psbt_input, input) in psbt.inputs.iter_mut().zip(psbt.unsigned_tx.input.iter()) {
//             let utxo = match lookup_output.remove(&input.previous_output) {
//                 Some(utxo) => utxo,
//                 None => continue,
//             };

//             match utxo {
//                 Utxo::Local(utxo) => {
//                     *psbt_input =
//                         match self.get_psbt_input(utxo, params.sighash, params.only_witness_utxo)
// {                             Ok(psbt_input) => psbt_input,
//                             Err(e) => match e {
//                                 CreateTxError::UnknownUtxo => psbt::Input {
//                                     sighash_type: params.sighash,
//                                     ..psbt::Input::default()
//                                 },
//                                 _ => return Err(e),
//                             },
//                         }
//                 }
//                 Utxo::Foreign {
//                     outpoint,
//                     psbt_input: foreign_psbt_input,
//                     ..
//                 } => {
//                     let is_taproot = foreign_psbt_input
//                         .witness_utxo
//                         .as_ref()
//                         .map(|txout| txout.script_pubkey.is_p2tr())
//                         .unwrap_or(false);
//                     if !is_taproot
//                         && !params.only_witness_utxo
//                         && foreign_psbt_input.non_witness_utxo.is_none()
//                     {
//                         return Err(CreateTxError::MissingNonWitnessUtxo(outpoint));
//                     }
//                     *psbt_input = *foreign_psbt_input;
//                 }
//             }
//         }

//         self.update_psbt_with_descriptor(&mut psbt)?;

//         Ok(psbt)
//     }

//     fn update_psbt_with_descriptor(&self, psbt: &mut Psbt) -> Result<(), MiniscriptPsbtError> {
//         // We need to borrow `psbt` mutably within the loops, so we have to allocate a vec for
// all         // the input utxos and outputs.
//         let utxos = (0..psbt.inputs.len())
//             .filter_map(|i| psbt.get_utxo_for(i).map(|utxo| (true, i, utxo)))
//             .chain(
//                 psbt.unsigned_tx
//                     .output
//                     .iter()
//                     .enumerate()
//                     .map(|(i, out)| (false, i, out.clone())),
//             )
//             .collect::<Vec<_>>();

//         // Try to figure out the keychain and derivation for every input and output.
//         for (is_input, index, out) in utxos.into_iter() {
//             if let Some(&(keychain, child)) =
//                 self.indexed_graph.index.index_of_spk(out.script_pubkey)
//             {
//                 let desc = self.public_descriptor(keychain);
//                 let desc = desc
//                     .at_derivation_index(child)
//                     .expect("child can't be hardened");

//                 if is_input {
//                     psbt.update_input_with_descriptor(index, &desc)
//                         .map_err(MiniscriptPsbtError::UtxoUpdate)?;
//                 } else {
//                     psbt.update_output_with_descriptor(index, &desc)
//                         .map_err(MiniscriptPsbtError::OutputUpdate)?;
//                 }
//             }
//         }

//         Ok(())
//     }

/// Deterministically generate a unique name given the descriptors defining the [`Wallet`].
///
/// Compatible with [`wallet_name_from_descriptor`].
pub fn wallet_name_from_descriptor<T>(
    descriptor: T,
    change_descriptor: Option<T>,
    network_kind: NetworkKind,
    secp: &SecpCtx,
) -> Result<String, descriptor::error::Error>
where
    T: IntoWalletDescriptor,
{
    // TODO: check descriptors contains only public keys
    let descriptor = descriptor
        .into_wallet_descriptor(secp, network_kind)?
        .0
        .to_string();
    let mut wallet_name = descriptor.split_once('#').unwrap().1.to_string();
    if let Some(change_descriptor) = change_descriptor {
        let change_descriptor = change_descriptor
            .into_wallet_descriptor(secp, network_kind)?
            .0
            .to_string();
        wallet_name.push_str(change_descriptor.split_once('#').unwrap().1);
    }

    Ok(wallet_name)
}

fn new_local_utxo<K>(
    keychain: K,
    derivation_index: u32,
    full_txo: FullTxOut<ConfirmationBlockTime>,
) -> LocalOutput<K>
where
    K: Clone,
{
    LocalOutput {
        outpoint: full_txo.outpoint,
        txout: full_txo.txout,
        is_spent: full_txo.spent_by.is_some(),
        chain_position: full_txo.chain_position,
        keychain,
        derivation_index,
    }
}

fn make_indexed_graph<K>(
    stage: &mut ChangeSet<K>,
    tx_graph_changeset: tx_graph::ChangeSet<ConfirmationBlockTime>,
    indexer_changeset: keychain_txout::ChangeSet,
    descriptors: BTreeMap<K, ExtendedDescriptor>,
    lookahead: u32,
    use_spk_cache: bool,
) -> Result<IndexedTxGraph<ConfirmationBlockTime, KeychainTxOutIndex<K>>, KeyRingError<K>>
where
    K: Ord + Clone + Debug,
{
    let (indexed_graph, changeset) = IndexedTxGraph::from_changeset(
        indexed_tx_graph::ChangeSet {
            tx_graph: tx_graph_changeset,
            indexer: indexer_changeset,
        },
        |idx_cs| -> Result<KeychainTxOutIndex<K>, KeyRingError<K>> {
            let mut idx = KeychainTxOutIndex::from_changeset(lookahead, use_spk_cache, idx_cs);

            for (keychain, desc) in descriptors {
                let _inserted = idx
                    .insert_descriptor(keychain.clone(), desc.clone())
                    .map_err(|e| {
                        use bdk_chain::indexer::keychain_txout::InsertDescriptorError;
                        match e {
                            InsertDescriptorError::DescriptorAlreadyAssigned { .. } => {
                                KeyRingError::DescAlreadyExists(Box::new(desc))
                            }
                            InsertDescriptorError::KeychainAlreadyAssigned { .. } => {
                                KeyRingError::KeychainAlreadyExists(keychain)
                            }
                        }
                    })?;
                assert!(
                    _inserted,
                    "this must be the first time we are seeing this descriptor"
                );
            }

            Ok(idx)
        },
    )?;
    stage.tx_graph.merge(changeset.tx_graph);
    stage.indexer.merge(changeset.indexer);
    Ok(indexed_graph)
}

/// Transforms a [`FeeRate`] to `f64` with unit as sat/vb.
#[macro_export]
#[doc(hidden)]
macro_rules! floating_rate {
    ($rate:expr) => {{
        use $crate::bitcoin::constants::WITNESS_SCALE_FACTOR;
        // sat_kwu / 250.0 -> sat_vb
        $rate.to_sat_per_kwu() as f64 / ((1000 / WITNESS_SCALE_FACTOR) as f64)
    }};
}

// #[macro_export]
// #[doc(hidden)]
// /// Macro for getting a [`Wallet`] for use in a doctest.
// macro_rules! doctest_wallet {
//     () => {{
//         use $crate::bitcoin::{BlockHash, Transaction, absolute, TxOut, Network, hashes::Hash};
//         use $crate::chain::{ConfirmationBlockTime, BlockId, TxGraph, tx_graph};
//         use $crate::{Update, KeychainKind, Wallet};
//         use $crate::test_utils::*;
//         let descriptor =
// "tr([73c5da0a/86'/0'/0'
// ]tprv8fMn4hSKPRC1oaCPqxDb1JWtgkpeiQvZhsr8W2xuy3GEMkzoArcAWTfJxYb6Wj8XNNDWEjfYKK4wGQXh3ZUXhDF2NcnsALpWTeSwarJt7Vc/
// 0/*)";         let change_descriptor =
// "tr([73c5da0a/86'/0'/0'
// ]tprv8fMn4hSKPRC1oaCPqxDb1JWtgkpeiQvZhsr8W2xuy3GEMkzoArcAWTfJxYb6Wj8XNNDWEjfYKK4wGQXh3ZUXhDF2NcnsALpWTeSwarJt7Vc/
// 1/*)";

//         let mut wallet = Wallet::create(descriptor, change_descriptor)
//             .network(Network::Regtest)
//             .create_wallet_no_persist()
//             .unwrap();
//         let address = wallet.peek_address(KeychainKind::External, 0).address;
//         let tx = Transaction {
//             version: transaction::Version::TWO,
//             lock_time: absolute::LockTime::ZERO,
//             input: vec![],
//             output: vec![TxOut {
//                 value: Amount::from_sat(500_000),
//                 script_pubkey: address.script_pubkey(),
//             }],
//         };
//         let txid = tx.compute_txid();
//         let block_id = BlockId { height: 500, hash: BlockHash::all_zeros() };
//         insert_checkpoint(&mut wallet, block_id);
//         insert_checkpoint(&mut wallet, BlockId { height: 1_000, hash: BlockHash::all_zeros() });
//         insert_tx(&mut wallet, tx);
//         let anchor = ConfirmationBlockTime {
//             confirmation_time: 50_000,
//             block_id,
//         };
//         insert_anchor(&mut wallet, txid, anchor);
//         wallet
//     }}
// }

#[cfg(test)]
mod test {
    use super::*;
    //     use crate::miniscript::Error::Unexpected;
    //     use crate::test_utils::get_test_tr_single_sig_xprv_and_change_desc;
    //     use crate::test_utils::insert_tx;
    use bdk_chain::DescriptorId;
    use core::str::FromStr;
    use miniscript::{Descriptor, DescriptorPublicKey};

    const DESCRIPTORS: [&str; 6] = [
    "wpkh(tpubDCzuCBKnZA5TNKhiJnASku7kq8Q4iqcVF82JV7mHo2NxWpXkLRbrJaGA5ToE7LCuWpcPErBbpDzbdWKN8aTdJzmRy1jQPmZvnqpwwDwCdy7/1/*)",
    "wpkh(tpubDCzuCBKnZA5TNKhiJnASku7kq8Q4iqcVF82JV7mHo2NxWpXkLRbrJaGA5ToE7LCuWpcPErBbpDzbdWKN8aTdJzmRy1jQPmZvnqpwwDwCdy7/2/*)",
    "tr(tpubDCzuCBKnZA5TNKhiJnASku7kq8Q4iqcVF82JV7mHo2NxWpXkLRbrJaGA5ToE7LCuWpcPErBbpDzbdWKN8aTdJzmRy1jQPmZvnqpwwDwCdy7/3/*)",
    "tr(tpubDCzuCBKnZA5TNKhiJnASku7kq8Q4iqcVF82JV7mHo2NxWpXkLRbrJaGA5ToE7LCuWpcPErBbpDzbdWKN8aTdJzmRy1jQPmZvnqpwwDwCdy7/4/*)",
    "pkh(tpubDCzuCBKnZA5TNKhiJnASku7kq8Q4iqcVF82JV7mHo2NxWpXkLRbrJaGA5ToE7LCuWpcPErBbpDzbdWKN8aTdJzmRy1jQPmZvnqpwwDwCdy7/5/*)",
    "pkh(tpubDCzuCBKnZA5TNKhiJnASku7kq8Q4iqcVF82JV7mHo2NxWpXkLRbrJaGA5ToE7LCuWpcPErBbpDzbdWKN8aTdJzmRy1jQPmZvnqpwwDwCdy7/6/*)"];

    /// Parse a descriptor string
    fn parse_descriptor(s: &str) -> Descriptor<DescriptorPublicKey> {
        Descriptor::parse_descriptor(&Secp256k1::new(), s)
            .expect("failed to parse descriptor")
            .0
    }

    fn test_keyring(desc_strs: impl IntoIterator<Item = &'static str>) -> KeyRing<DescriptorId> {
        let mut desc_strs = desc_strs.into_iter();
        let desc = parse_descriptor(desc_strs.next().unwrap());
        let mut keyring = KeyRing::new(Network::Testnet4, desc.descriptor_id(), desc).unwrap();
        for desc_str in desc_strs {
            let desc = parse_descriptor(desc_str);
            let _ = keyring.add_descriptor(desc.descriptor_id(), desc);
        }
        keyring
    }

    #[test]
    fn correct_address_is_revealed() {
        let mut wallet = Wallet::create(test_keyring(DESCRIPTORS))
            .create_wallet_no_persist()
            .unwrap();
        let addrinfo = wallet
            .reveal_next_address(parse_descriptor(DESCRIPTORS[1]).descriptor_id())
            .unwrap();
        assert_eq!(
            addrinfo.address.into_unchecked(),
            Address::from_str("tb1qun8txyd3p4xgts6y6lj8h2dcxk20s487ll7ss3").unwrap()
        );
        let addrinfo = wallet
            .reveal_next_address(parse_descriptor(DESCRIPTORS[2]).descriptor_id())
            .unwrap();
        assert_eq!(
            addrinfo.address.into_unchecked(),
            Address::from_str("tb1pnz3jex4wnz88e46rfzckpd9xyvdde8h2hnes4wrllkhygump8c2se9rusg")
                .unwrap()
        );
        let addrinfo = wallet
            .reveal_next_address(parse_descriptor(DESCRIPTORS[3]).descriptor_id())
            .unwrap();
        assert_eq!(
            addrinfo.address.into_unchecked(),
            Address::from_str("tb1pv6hmnghp0wtxzeqsvshdq4ennvmqg3eq78vluvzvfkqtmtd5e49q8zht5v")
                .unwrap()
        );
        let addrinfo = wallet
            .reveal_next_address(parse_descriptor(DESCRIPTORS[4]).descriptor_id())
            .unwrap();
        assert_eq!(
            addrinfo.address.into_unchecked(),
            Address::from_str("n3TJoFpLPBMGisVYHUGcEBwd9d1FVBwbJQ").unwrap()
        );
        let addrinfo = wallet
            .reveal_next_address(parse_descriptor(DESCRIPTORS[5]).descriptor_id())
            .unwrap();
        assert_eq!(
            addrinfo.address.into_unchecked(),
            Address::from_str("mq2r39CD8ZnMqyuytQq2zPa1sfHTT6Rjo8").unwrap()
        );
    }

    //     #[test]
    //     fn not_duplicated_utxos_across_optional_and_required() {
    //         let (external_desc, internal_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    // // Create new wallet.
    // let mut wallet = Wallet::create(external_desc, internal_desc)
    //     .network(Network::Testnet)
    //     .create_wallet_no_persist()
    //     .unwrap();

    //         let two_output_tx = Transaction {
    //             input: vec![],
    //             output: vec![
    //                 TxOut {
    //                     script_pubkey: wallet
    //                         .next_unused_address(KeychainKind::External)
    //                         .script_pubkey(),
    //                     value: Amount::from_sat(25_000),
    //                 },
    //                 TxOut {
    //                     script_pubkey: wallet
    //                         .next_unused_address(KeychainKind::External)
    //                         .script_pubkey(),
    //                     value: Amount::from_sat(75_000),
    //                 },
    //             ],
    //             version: transaction::Version::non_standard(0),
    //             lock_time: absolute::LockTime::ZERO,
    //         };

    //         let txid = two_output_tx.compute_txid();
    //         insert_tx(&mut wallet, two_output_tx);

    // let outpoint = OutPoint { txid, vout: 0 };
    // let mut builder = wallet.build_tx();
    // builder.add_utxo(outpoint).expect("should add local utxo");
    // let params = builder.params.clone();
    // // enforce selection of first output in transaction
    // let received = wallet.filter_utxos(&params, wallet.latest_checkpoint().block_id().height);
    // // Notice expected doesn't include the first output from two_output_tx as it should be
    // // filtered out.
    // let expected = vec![wallet
    //     .get_utxo(OutPoint { txid, vout: 1 })
    //     .map(|utxo| WeightedUtxo {
    //         satisfaction_weight: wallet
    //             .public_descriptor(utxo.keychain)
    //             .max_weight_to_satisfy()
    //             .unwrap(),
    //         utxo: Utxo::Local(utxo),
    //     })
    //     .unwrap()];

    //         assert_eq!(expected, received);
    //     }

    //     #[test]
    //     fn test_create_two_path_wallet() {
    //         let two_path_descriptor =
    // "wpkh([9a6a2580/84'/1'/0'
    // ]tpubDDnGNapGEY6AZAdQbfRJgMg9fvz8pUBrLwvyvUqEgcUfgzM6zc2eVK4vY9x9L5FJWdX8WumXuLEDV5zDZnTfbn87vLe9XceCFwTu9so9Kks/
    // <0;1>/*)";

    // TODO PR #318: We supported creating wallets from multi-path descriptors
    //               and had tests here. These don't belong here anymore but we should make sure we
    //               have tests for them in the KeyRing tests.
}

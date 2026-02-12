//! Arbitrary types for structure-aware fuzzing
//!
//! This module provides wrapper types that implement the Arbitrary trait
//! for efficient structure-aware fuzzing of BDK wallet components.

use arbitrary::{Arbitrary, Result, Unstructured};
use bdk_wallet::bitcoin::{
    absolute::LockTime, hashes::Hash, psbt::PsbtSighashType, transaction::Version,
    Amount, BlockHash, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid,
};
use bdk_wallet::{
    chain::{BlockId, ConfirmationBlockTime},
    signer::TapLeavesOptions, KeychainKind, SignOptions, TxOrdering, Update, Wallet,
};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

/// A fuzzed transaction ID
#[derive(Arbitrary, Debug, Clone)]
pub struct FuzzedTxid([u8; 32]);

impl FuzzedTxid {
    pub fn into_txid(self) -> Txid {
        Txid::from_byte_array(self.0)
    }
}

/// A fuzzed block hash
#[derive(Arbitrary, Debug, Clone)]
pub struct FuzzedBlockHash([u8; 32]);

impl FuzzedBlockHash {
    pub fn into_block_hash(self) -> BlockHash {
        BlockHash::from_byte_array(self.0)
    }
}

/// A fuzzed outpoint (transaction output reference)
#[derive(Arbitrary, Debug, Clone)]
pub struct FuzzedOutPoint {
    txid: FuzzedTxid,
    vout: u32,
}

impl FuzzedOutPoint {
    pub fn into_outpoint(self) -> OutPoint {
        OutPoint::new(self.txid.into_txid(), self.vout)
    }
}

/// A fuzzed amount in satoshis with reasonable constraints
#[derive(Debug, Clone)]
pub struct FuzzedAmount(u64);

impl Arbitrary<'_> for FuzzedAmount {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Generate amounts between 0 and 21 million BTC in satoshis
        // Use smaller amounts more frequently for better test coverage
        let max_sats = 21_000_000 * 100_000_000u64;
        let amount = if u.ratio(9, 10)? {
            // 90% of the time use smaller amounts (up to 1000 BTC)
            u.int_in_range(0..=100_000_000_000)?
        } else {
            // 10% of the time use any amount up to max supply
            u.int_in_range(0..=max_sats)?
        };
        Ok(FuzzedAmount(amount))
    }
}

impl FuzzedAmount {
    pub fn into_amount(self) -> Amount {
        Amount::from_sat(self.0)
    }

    pub fn as_sats(&self) -> u64 {
        self.0
    }
}

/// A fuzzed script with size constraints
#[derive(Debug, Clone)]
pub struct FuzzedScript(Vec<u8>);

impl Arbitrary<'_> for FuzzedScript {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Generate scripts with reasonable size limits
        // Most scripts are small, occasionally generate larger ones
        let max_len = if u.ratio(9, 10)? {
            100  // 90% of the time, small scripts
        } else {
            520  // 10% of the time, up to standard max script size
        };

        let len = u.int_in_range(0..=max_len)?;
        let mut bytes = vec![0u8; len];
        u.fill_buffer(&mut bytes)?;
        Ok(FuzzedScript(bytes))
    }
}

impl FuzzedScript {
    pub fn into_script(self) -> ScriptBuf {
        ScriptBuf::from_bytes(self.0)
    }
}

/// Wallet actions that can be performed during fuzzing
#[derive(Arbitrary, Debug, Clone)]
pub enum FuzzedWalletAction {
    /// Apply an update to the wallet
    ApplyUpdate,
    /// Create and sign a transaction
    CreateTx,
    /// Persist wallet state and reload it
    PersistAndLoad,
}

/// Fuzzed signing options for wallet operations
#[derive(Debug, Clone)]
pub struct FuzzedSignOptions {
    pub trust_witness_utxo: bool,
    pub assume_height: Option<u32>,
    pub allow_all_sighashes: bool,
    pub try_finalize: bool,
    pub tap_leaves_options: FuzzedTapLeavesOptions,
    pub sign_with_tap_internal_key: bool,
    pub allow_grinding: bool,
}

impl Arbitrary<'_> for FuzzedSignOptions {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(FuzzedSignOptions {
            trust_witness_utxo: u.arbitrary()?,
            assume_height: if u.arbitrary()? {
                Some(u.int_in_range(0..=2_000_000)?)  // Reasonable block height range
            } else {
                None
            },
            allow_all_sighashes: u.arbitrary()?,
            try_finalize: u.arbitrary()?,
            tap_leaves_options: u.arbitrary()?,
            sign_with_tap_internal_key: u.arbitrary()?,
            allow_grinding: u.arbitrary()?,
        })
    }
}

impl FuzzedSignOptions {
    pub fn into_sign_options(self) -> SignOptions {
        SignOptions {
            trust_witness_utxo: self.trust_witness_utxo,
            assume_height: self.assume_height,
            allow_all_sighashes: self.allow_all_sighashes,
            try_finalize: self.try_finalize,
            tap_leaves_options: self.tap_leaves_options.into_tap_leaves_options(),
            sign_with_tap_internal_key: self.sign_with_tap_internal_key,
            allow_grinding: self.allow_grinding,
        }
    }
}

/// Taproot leaves signing options
#[derive(Arbitrary, Debug, Clone)]
pub enum FuzzedTapLeavesOptions {
    /// Sign all taproot leaves
    All,
    /// Don't sign any taproot leaves
    None,
    // TODO: Add Include/Exclude variants with specific leaf hashes when needed
}

impl FuzzedTapLeavesOptions {
    pub fn into_tap_leaves_options(self) -> TapLeavesOptions {
        match self {
            FuzzedTapLeavesOptions::All => TapLeavesOptions::All,
            FuzzedTapLeavesOptions::None => TapLeavesOptions::None,
        }
    }
}

/// Options for building transactions
#[derive(Debug, Clone)]
pub struct FuzzedTxBuilderOptions {
    pub fee_rate: Option<u64>,        // Satoshis per vbyte
    pub fee_absolute: Option<u64>,    // Absolute fee in satoshis
    pub manually_selected_only: bool,
    pub sighash: Option<PsbtSighashType>,
    pub ordering: FuzzedTxOrdering,
    pub locktime: Option<u32>,
    pub version: Option<i32>,
    pub do_not_spend_change: bool,
    pub only_spend_change: bool,
    pub only_witness_utxo: bool,
    pub include_output_redeem_witness_script: bool,
    pub add_global_xpubs: bool,
    pub drain_wallet: bool,
    pub allow_dust: bool,
}

impl Arbitrary<'_> for FuzzedTxBuilderOptions {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(FuzzedTxBuilderOptions {
            fee_rate: if u.ratio(1, 3)? {
                // Use reasonable fee rates (1-1000 sat/vb)
                Some(u.int_in_range(1..=1000)?)
            } else {
                None
            },
            fee_absolute: if u.ratio(1, 10)? {
                // Absolute fees up to 0.01 BTC
                Some(u.int_in_range(0..=1_000_000)?)
            } else {
                None
            },
            manually_selected_only: u.arbitrary()?,
            sighash: if u.ratio(1, 10)? {
                // Occasionally set custom sighash
                Some(PsbtSighashType::from_u32(u.int_in_range(0..=0x83)?))
            } else {
                None
            },
            ordering: u.arbitrary()?,
            locktime: if u.ratio(1, 5)? {
                Some(u.arbitrary()?)
            } else {
                None
            },
            version: if u.ratio(1, 10)? {
                Some(u.int_in_range(1..=2)?)
            } else {
                None
            },
            do_not_spend_change: u.ratio(1, 20)?,  // Rare option
            only_spend_change: u.ratio(1, 20)?,     // Rare option
            only_witness_utxo: u.arbitrary()?,
            include_output_redeem_witness_script: u.arbitrary()?,
            add_global_xpubs: u.arbitrary()?,
            drain_wallet: u.ratio(1, 10)?,
            allow_dust: u.ratio(1, 5)?,
        })
    }
}

/// Transaction ordering options
#[derive(Arbitrary, Debug, Clone)]
pub enum FuzzedTxOrdering {
    Shuffle,
    Untouched,
    // BIP69 could be added here if needed
}

impl FuzzedTxOrdering {
    pub fn into_tx_ordering(self) -> TxOrdering {
        match self {
            FuzzedTxOrdering::Shuffle => TxOrdering::Shuffle,
            FuzzedTxOrdering::Untouched => TxOrdering::Untouched,
        }
    }
}

/// A fuzzed transaction input
#[derive(Debug, Clone)]
pub struct FuzzedTxInput {
    pub previous_output: FuzzedOutPoint,
    pub sequence: u32,
}

impl Arbitrary<'_> for FuzzedTxInput {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(FuzzedTxInput {
            previous_output: u.arbitrary()?,
            sequence: if u.ratio(1, 10)? {
                // 10% of the time use specific sequences for RBF/CSV
                *u.choose(&[0xfffffffd, 0xfffffffe, 0xffffffff, 0, 144, 288])?
            } else {
                u.arbitrary()?
            },
        })
    }
}

impl FuzzedTxInput {
    pub fn into_tx_in(self) -> TxIn {
        TxIn {
            previous_output: self.previous_output.into_outpoint(),
            sequence: Sequence(self.sequence),
            ..Default::default()
        }
    }
}

/// A fuzzed transaction output
#[derive(Debug, Clone)]
pub struct FuzzedTxOutput {
    pub value: FuzzedAmount,
    pub script_pubkey: FuzzedScript,
}

impl Arbitrary<'_> for FuzzedTxOutput {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(FuzzedTxOutput {
            value: u.arbitrary()?,
            script_pubkey: u.arbitrary()?,
        })
    }
}

impl FuzzedTxOutput {
    pub fn into_tx_out(self) -> TxOut {
        TxOut {
            value: self.value.into_amount(),
            script_pubkey: self.script_pubkey.into_script(),
        }
    }

    /// Create a transaction output using a wallet address
    pub fn from_wallet_address(wallet: &mut Wallet, amount: FuzzedAmount, is_change: bool) -> TxOut {
        let script = if is_change {
            wallet.next_unused_address(KeychainKind::Internal).script_pubkey()
        } else {
            wallet.next_unused_address(KeychainKind::External).script_pubkey()
        };
        TxOut {
            value: amount.into_amount(),
            script_pubkey: script,
        }
    }
}

/// A fuzzed transaction
#[derive(Debug, Clone)]
pub struct FuzzedTransaction {
    pub version: i32,
    pub lock_time: u32,
    pub inputs: Vec<FuzzedTxInput>,
    pub outputs: Vec<FuzzedTxOutput>,
}

impl Arbitrary<'_> for FuzzedTransaction {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Limit transaction size for performance
        let num_inputs = u.int_in_range(0..=10)?;
        let num_outputs = u.int_in_range(0..=10)?;

        let mut inputs = Vec::with_capacity(num_inputs);
        for _ in 0..num_inputs {
            inputs.push(u.arbitrary()?);
        }

        let mut outputs = Vec::with_capacity(num_outputs);
        for _ in 0..num_outputs {
            outputs.push(u.arbitrary()?);
        }

        Ok(FuzzedTransaction {
            version: u.int_in_range(1..=2)?,
            lock_time: u.arbitrary()?,
            inputs,
            outputs,
        })
    }
}

impl FuzzedTransaction {
    pub fn into_transaction(self) -> Transaction {
        Transaction {
            version: Version(self.version),
            lock_time: LockTime::from_consensus(self.lock_time),
            input: self.inputs.into_iter().map(|i| i.into_tx_in()).collect(),
            output: self.outputs.into_iter().map(|o| o.into_tx_out()).collect(),
        }
    }

    /// Create a transaction with wallet-aware outputs
    pub fn into_transaction_with_wallet(self, wallet: &mut Wallet) -> Transaction {
        let outputs: Vec<TxOut> = self.outputs
            .into_iter()
            .enumerate()
            .map(|(i, out)| {
                // Make some outputs spendable by the wallet
                if i % 3 == 0 {
                    FuzzedTxOutput::from_wallet_address(
                        wallet,
                        out.value,
                        i % 2 == 0
                    )
                } else {
                    out.into_tx_out()
                }
            })
            .collect();

        Transaction {
            version: Version(self.version),
            lock_time: LockTime::from_consensus(self.lock_time),
            input: self.inputs.into_iter().map(|i| i.into_tx_in()).collect(),
            output: outputs,
        }
    }
}

/// A fuzzed confirmation anchor
#[derive(Debug, Clone)]
pub struct FuzzedAnchor {
    pub block_height: u32,
    pub block_hash: FuzzedBlockHash,
    pub confirmation_time: u64,
}

impl Arbitrary<'_> for FuzzedAnchor {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(FuzzedAnchor {
            block_height: u.int_in_range(0..=2_000_000)?,
            block_hash: u.arbitrary()?,
            confirmation_time: u.int_in_range(0..=2_000_000_000)?,  // Unix timestamp range
        })
    }
}

impl FuzzedAnchor {
    pub fn into_confirmation_block_time(self) -> ConfirmationBlockTime {
        ConfirmationBlockTime {
            block_id: BlockId {
                height: self.block_height,
                hash: self.block_hash.into_block_hash(),
            },
            confirmation_time: self.confirmation_time,
        }
    }
}

/// A fuzzed transaction update
#[derive(Debug)]
pub struct FuzzedTxUpdate {
    pub txs: Vec<FuzzedTransaction>,
    pub txouts: Vec<(FuzzedOutPoint, FuzzedTxOutput)>,
    pub anchors: Vec<(FuzzedAnchor, FuzzedTxid)>,
    pub seen_ats: Vec<(FuzzedTxid, u64)>,
    pub evicted_ats: Vec<(FuzzedTxid, u64)>,
}

impl Arbitrary<'_> for FuzzedTxUpdate {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Limit sizes for performance
        let num_txs = u.int_in_range(0..=5)?;
        let num_txouts = u.int_in_range(0..=10)?;
        let num_anchors = u.int_in_range(0..=5)?;
        let num_seen = u.int_in_range(0..=5)?;
        let num_evicted = u.int_in_range(0..=3)?;

        Ok(FuzzedTxUpdate {
            txs: (0..num_txs).map(|_| u.arbitrary()).collect::<Result<_>>()?,
            txouts: (0..num_txouts).map(|_| Ok((u.arbitrary()?, u.arbitrary()?))).collect::<Result<_>>()?,
            anchors: (0..num_anchors).map(|_| Ok((u.arbitrary()?, u.arbitrary()?))).collect::<Result<_>>()?,
            seen_ats: (0..num_seen).map(|_| Ok((u.arbitrary()?, u.int_in_range(0..=2_000_000_000)?))).collect::<Result<_>>()?,
            evicted_ats: (0..num_evicted).map(|_| Ok((u.arbitrary()?, u.int_in_range(0..=2_000_000_000)?))).collect::<Result<_>>()?,
        })
    }
}

impl FuzzedTxUpdate {
    pub fn into_tx_update(self, wallet: &mut Wallet) -> bdk_wallet::chain::TxUpdate<ConfirmationBlockTime> {
        let txs: Vec<Arc<Transaction>> = self.txs
            .into_iter()
            .map(|tx| Arc::new(tx.into_transaction_with_wallet(wallet)))
            .collect();

        let txouts: BTreeMap<OutPoint, TxOut> = self.txouts
            .into_iter()
            .map(|(op, out)| (op.into_outpoint(), out.into_tx_out()))
            .collect();

        let anchors: BTreeSet<(ConfirmationBlockTime, Txid)> = self.anchors
            .into_iter()
            .map(|(anchor, txid)| (anchor.into_confirmation_block_time(), txid.into_txid()))
            .collect();

        let seen_ats: HashSet<(Txid, u64)> = self.seen_ats
            .into_iter()
            .map(|(txid, time)| (txid.into_txid(), time))
            .collect();

        let evicted_ats: HashSet<(Txid, u64)> = self.evicted_ats
            .into_iter()
            .map(|(txid, time)| (txid.into_txid(), time))
            .collect();

        let mut update = bdk_wallet::chain::TxUpdate::<ConfirmationBlockTime>::default();
        update.txs = txs;
        update.txouts = txouts;
        update.anchors = anchors;
        update.seen_ats = seen_ats;
        update.evicted_ats = evicted_ats;
        update
    }
}

/// A fuzzed checkpoint for the blockchain
#[derive(Debug, Clone)]
pub struct FuzzedCheckpoint {
    pub blocks_to_add: Vec<(u32, FuzzedBlockHash)>,
}

impl Arbitrary<'_> for FuzzedCheckpoint {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Add 0-10 blocks to the chain
        let num_blocks = u.int_in_range(0..=10)?;
        let mut blocks = Vec::with_capacity(num_blocks);

        for i in 0..num_blocks {
            let height = i as u32;  // Heights will be relative to current tip
            blocks.push((height, u.arbitrary()?));
        }

        Ok(FuzzedCheckpoint { blocks_to_add: blocks })
    }
}

/// A complete fuzzed update for the wallet
#[derive(Debug)]
pub struct FuzzedUpdate {
    pub last_active_indices: Vec<(KeychainKind, u32)>,
    pub tx_update: FuzzedTxUpdate,
    pub checkpoint: Option<FuzzedCheckpoint>,
}

impl Arbitrary<'_> for FuzzedUpdate {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Generate keychain indices
        let mut indices = Vec::new();
        for keychain in [KeychainKind::External, KeychainKind::Internal] {
            if u.ratio(2, 3)? {  // 66% chance of including indices
                let num_indices = u.int_in_range(0..=20)?;
                for idx in 0..num_indices {
                    indices.push((keychain, idx));
                }
            }
        }

        Ok(FuzzedUpdate {
            last_active_indices: indices,
            tx_update: u.arbitrary()?,
            checkpoint: if u.ratio(1, 2)? {
                Some(u.arbitrary()?)
            } else {
                None
            },
        })
    }
}

impl FuzzedUpdate {
    pub fn into_update(self, wallet: &mut Wallet) -> Update {
        let last_active_indices: BTreeMap<KeychainKind, u32> = self.last_active_indices
            .into_iter()
            .collect();

        let chain = self.checkpoint.map(|checkpoint| {
            let mut tip = wallet.latest_checkpoint();
            let tip_height = tip.height();

            for (relative_height, hash) in checkpoint.blocks_to_add {
                let height = tip_height + relative_height + 1;
                let block_id = BlockId {
                    height,
                    hash: hash.into_block_hash(),
                };
                // Ignore errors from invalid checkpoints
                tip = match tip.clone().push(block_id) {
                    Ok(new_tip) => new_tip,
                    Err(old_tip) => old_tip,
                };
            }
            tip
        });

        Update {
            last_active_indices,
            tx_update: self.tx_update.into_tx_update(wallet),
            chain,
        }
    }
}
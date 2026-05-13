//! Fuzz target: apply arbitrary blockchain updates to a fresh wallet.
//!
//! Exercises `Wallet::apply_update` with structured random inputs including
//! transactions, confirmation anchors, mempool timestamps, and chain checkpoints.
//! Any panic is a bug; errors returned by `apply_update` are expected and ignored.
#![no_main]

use arbitrary::Arbitrary;
use bdk_wallet_fuzz::{create_wallet, p2wpkh_script};
use libfuzzer_sys::fuzz_target;

use bdk_wallet::{
    bitcoin::{
        absolute::LockTime, hashes::Hash, transaction::Version, Amount, BlockHash, OutPoint,
        Sequence, Transaction, TxIn, TxOut, Txid,
    },
    chain::{BlockId, CheckPoint, ConfirmationBlockTime, TxUpdate},
    KeychainKind, Update,
};
use std::{collections::BTreeMap, sync::Arc};

/// Maximum valid satoshi value to avoid arithmetic panics.
const MAX_SATS: u64 = 21_000_000 * 100_000_000;

#[derive(Arbitrary, Debug)]
struct FuzzedUpdate {
    txs: Vec<FuzzedTx>,
    /// Each entry: (tx index byte, block height) — index is taken mod txs.len().
    anchors: Vec<(u8, u32)>,
    /// Each entry: (tx index byte, unix timestamp).
    seen_ats: Vec<(u8, u64)>,
    last_active_ext: Option<u16>,
    last_active_int: Option<u16>,
    /// Sorted, deduplicated heights used to build a chain checkpoint.
    chain_heights: Vec<u32>,
}

#[derive(Arbitrary, Debug)]
struct FuzzedTx {
    version: bool, // false → version 1, true → version 2
    locktime: u32,
    /// (txid bytes, output index, sequence)
    inputs: Vec<([u8; 32], u32, u32)>,
    /// (satoshi value, 20-byte script hash)
    outputs: Vec<(u64, [u8; 20])>,
}

impl FuzzedTx {
    fn into_transaction(self) -> Transaction {
        let input = self
            .inputs
            .into_iter()
            .map(|(txid_bytes, vout, seq)| TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_byte_array(txid_bytes),
                    vout,
                },
                sequence: Sequence(seq),
                ..Default::default()
            })
            .collect();

        let output: Vec<TxOut> = self
            .outputs
            .into_iter()
            .filter_map(|(sats, hash)| {
                // Clamp to valid range; skip zero-value outputs.
                if sats == 0 || sats > MAX_SATS {
                    return None;
                }
                Some(TxOut {
                    value: Amount::from_sat(sats),
                    script_pubkey: p2wpkh_script(hash),
                })
            })
            .collect();

        Transaction {
            version: Version(if self.version { 2 } else { 1 }),
            lock_time: LockTime::from_consensus(self.locktime),
            input,
            output,
        }
    }
}

fuzz_target!(|input: FuzzedUpdate| {
    let mut wallet = create_wallet();

    if input.txs.is_empty() {
        return;
    }

    let genesis_hash = wallet.latest_checkpoint().hash();

    let txs: Vec<Arc<Transaction>> = input
        .txs
        .into_iter()
        .map(|t| Arc::new(t.into_transaction()))
        .collect();

    let mut tx_update = TxUpdate::<ConfirmationBlockTime>::default();
    tx_update.txs = txs.clone();

    for (idx_byte, height) in input.anchors {
        let idx = idx_byte as usize % txs.len();
        let txid = txs[idx].compute_txid();
        tx_update.anchors.insert((
            ConfirmationBlockTime {
                block_id: BlockId {
                    height,
                    hash: BlockHash::all_zeros(),
                },
                confirmation_time: 1_600_000_000,
            },
            txid,
        ));
    }

    for (idx_byte, ts) in input.seen_ats {
        let idx = idx_byte as usize % txs.len();
        let txid = txs[idx].compute_txid();
        tx_update.seen_ats.insert((txid, ts));
    }

    let chain = if !input.chain_heights.is_empty() {
        let mut heights = input.chain_heights;
        heights.sort_unstable();
        heights.dedup();

        let mut block_ids = vec![BlockId {
            height: 0,
            hash: genesis_hash,
        }];
        block_ids.extend(heights.into_iter().map(|h| BlockId {
            height: h,
            hash: BlockHash::all_zeros(),
        }));

        CheckPoint::from_block_ids(block_ids).ok()
    } else {
        None
    };

    let mut last_active_indices = BTreeMap::new();
    if let Some(idx) = input.last_active_ext {
        last_active_indices.insert(KeychainKind::External, idx as u32);
    }
    if let Some(idx) = input.last_active_int {
        last_active_indices.insert(KeychainKind::Internal, idx as u32);
    }

    let update = Update {
        tx_update,
        chain,
        last_active_indices,
    };

    let _ = wallet.apply_update(update);

    // Invariant: total confirmed balance never exceeds the 21M BTC supply cap.
    assert!(wallet.balance().total().to_sat() <= MAX_SATS);
});

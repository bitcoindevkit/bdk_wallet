//! User facing wallet events.

use crate::Wallet;
use crate::collections::BTreeMap;
use crate::wallet::ChainPosition::{Confirmed, Unconfirmed};
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitcoin::{Transaction, Txid};
use chain::{BlockId, ChainPosition, ConfirmationBlockTime};
use core::fmt;

/// Events representing changes to wallet transactions.
///
/// Returned after calling
/// [`Wallet::apply_update_events`](crate::wallet::Wallet::apply_update_events).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WalletEvent {
    /// The latest chain tip known to the wallet changed.
    ChainTipChanged {
        /// Previous chain tip.
        old_tip: BlockId,
        /// New chain tip.
        new_tip: BlockId,
    },
    /// A transaction is now confirmed.
    ///
    /// If the transaction was previously unconfirmed `old_block_time` will be `None`.
    ///
    /// If a confirmed transaction is now re-confirmed in a new block `old_block_time` will contain
    /// the block id and the time it was previously confirmed. This can happen after a chain
    /// reorg.
    TxConfirmed {
        /// Transaction id.
        txid: Txid,
        /// Transaction.
        tx: Arc<Transaction>,
        /// Confirmation block time.
        block_time: ConfirmationBlockTime,
        /// Old confirmation block and time if previously confirmed in a different block.
        old_block_time: Option<ConfirmationBlockTime>,
    },
    /// A transaction is now unconfirmed.
    ///
    /// If the transaction is first seen in the mempool `old_block_time` will be `None`.
    ///
    /// If a previously confirmed transaction is now seen in the mempool `old_block_time` will
    /// contain the block id and the time it was previously confirmed. This can happen after a
    /// chain reorg.
    TxUnconfirmed {
        /// Transaction id.
        txid: Txid,
        /// Transaction.
        tx: Arc<Transaction>,
        /// Old confirmation block and time, if previously confirmed.
        old_block_time: Option<ConfirmationBlockTime>,
    },
    /// An unconfirmed transaction was replaced.
    ///
    /// This can happen after an RBF is broadcast or if a third party double spends an input of
    /// a received payment transaction before it is confirmed.
    ///
    /// The conflicts field contains the txid and vin (in which it conflicts) of the conflicting
    /// transactions.
    TxReplaced {
        /// Transaction id.
        txid: Txid,
        /// Transaction.
        tx: Arc<Transaction>,
        /// Conflicting transaction ids.
        conflicts: Vec<(usize, Txid)>,
    },
    /// An unconfirmed transaction was rejected and not applied to the wallet.
    ///
    /// The transaction spends outputs owned by the wallet, but the signatures (or other
    /// satisfactions) for those inputs are missing or invalid. Such a transaction can't be valid,
    /// so the chain source that returned it is likely malicious: it may be trying to make the
    /// wallet replace a real transaction with one that pays an attacker.
    ///
    /// A legitimate transaction can also be rejected if it spends a wallet-owned taproot output
    /// together with foreign inputs whose prevouts are unknown to the wallet (e.g. a payjoin or
    /// coinjoin): taproot signatures commit to all the prevouts, so it can't be verified. It is
    /// accepted once confirmed, or once applied again with the missing prevouts. Electrum users
    /// avoid this by syncing with `fetch_prev_txouts = true`; see
    /// [`Wallet::apply_update`](crate::Wallet::apply_update) for other chain sources. Use
    /// `reason` to tell the two cases apart.
    ///
    /// Rejections are not persisted: a chain source that keeps returning the transaction causes
    /// it to be rejected, and reported, on every sync.
    TxRejected {
        /// Transaction id.
        txid: Txid,
        /// Transaction.
        tx: Arc<Transaction>,
        /// Why the transaction was rejected.
        reason: TxRejectedReason,
    },
    /// Unconfirmed transaction dropped.
    ///
    /// The transaction was dropped from the local mempool. This is generally due to the fee rate
    /// being too low. The transaction can still reappear in the mempool in the future resulting in
    /// a [`WalletEvent::TxUnconfirmed`] event.
    TxDropped {
        /// Transaction id.
        txid: Txid,
        /// Transaction.
        tx: Arc<Transaction>,
    },
}

/// Why an unconfirmed transaction was rejected, see [`WalletEvent::TxRejected`].
///
/// Only the first wallet-owned input that failed verification is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TxRejectedReason {
    /// A wallet-owned input does not validly satisfy its prevout script: its signatures (or other
    /// satisfactions) are missing or invalid.
    ///
    /// The transaction can't be valid. The chain source that returned it is likely malicious.
    InvalidInput {
        /// Index of the offending input.
        input_index: usize,
    },
    /// A wallet-owned taproot input fails to verify, and the prevouts of some other inputs are
    /// unknown to the wallet.
    ///
    /// Taproot signatures (other than `SIGHASH_ANYONECANPAY` ones) commit to the prevouts of all
    /// the inputs, so the input may be validly signed but can't be verified. This is expected for
    /// legitimate payjoins or coinjoins synced without their foreign prevouts (e.g. Electrum with
    /// `fetch_prev_txouts = false`), but a forged transaction is reported the same way.
    MissingPrevouts {
        /// Index of the input that could not be verified.
        input_index: usize,
    },
}

impl fmt::Display for TxRejectedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { input_index } => write!(
                f,
                "input {input_index} spends a wallet output without a valid satisfaction"
            ),
            Self::MissingPrevouts { input_index } => write!(
                f,
                "input {input_index} spends a wallet taproot output and can't be verified without the missing prevouts"
            ),
        }
    }
}

/// Generate `WalletEvent`s by comparing the chain tip and wallet transactions before and after
/// updating the state of the `Wallet`.
pub(crate) fn wallet_events(
    wallet: &Wallet,
    chain_tip1: BlockId,
    chain_tip2: BlockId,
    wallet_txs1: BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>,
    wallet_txs2: BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>,
) -> Vec<WalletEvent> {
    let mut events: Vec<WalletEvent> = Vec::new();

    // find chain tip change
    if chain_tip1 != chain_tip2 {
        events.push(WalletEvent::ChainTipChanged {
            old_tip: chain_tip1,
            new_tip: chain_tip2,
        });
    }

    // find transaction canonical status changes
    wallet_txs2.iter().for_each(|(txid2, (tx2, pos2))| {
        if let Some((tx1, pos1)) = wallet_txs1.get(txid2) {
            debug_assert_eq!(tx1.compute_txid(), *txid2);
            match (pos1, pos2) {
                (Unconfirmed { .. }, Confirmed { anchor, .. }) => {
                    events.push(WalletEvent::TxConfirmed {
                        txid: *txid2,
                        tx: tx2.clone(),
                        block_time: *anchor,
                        old_block_time: None,
                    });
                }
                (Confirmed { anchor, .. }, Unconfirmed { .. }) => {
                    events.push(WalletEvent::TxUnconfirmed {
                        txid: *txid2,
                        tx: tx2.clone(),
                        old_block_time: Some(*anchor),
                    });
                }
                (
                    Confirmed {
                        anchor: anchor1, ..
                    },
                    Confirmed {
                        anchor: anchor2, ..
                    },
                ) => {
                    if *anchor1 != *anchor2 {
                        events.push(WalletEvent::TxConfirmed {
                            txid: *txid2,
                            tx: tx2.clone(),
                            block_time: *anchor2,
                            old_block_time: Some(*anchor1),
                        });
                    }
                }
                (Unconfirmed { .. }, Unconfirmed { .. }) => {
                    // do nothing if still unconfirmed
                }
            }
        } else {
            match pos2 {
                Confirmed { anchor, .. } => {
                    events.push(WalletEvent::TxConfirmed {
                        txid: *txid2,
                        tx: tx2.clone(),
                        block_time: *anchor,
                        old_block_time: None,
                    });
                }
                Unconfirmed { .. } => {
                    events.push(WalletEvent::TxUnconfirmed {
                        txid: *txid2,
                        tx: tx2.clone(),
                        old_block_time: None,
                    });
                }
            }
        }
    });

    // find tx that are no longer canonical
    wallet_txs1.iter().for_each(|(txid1, (tx1, _))| {
        if !wallet_txs2.contains_key(txid1) {
            let conflicts = wallet.tx_graph().direct_conflicts(tx1).collect::<Vec<_>>();
            if !conflicts.is_empty() {
                events.push(WalletEvent::TxReplaced {
                    txid: *txid1,
                    tx: tx1.clone(),
                    conflicts,
                });
            } else {
                events.push(WalletEvent::TxDropped {
                    txid: *txid1,
                    tx: tx1.clone(),
                });
            }
        }
    });

    events
}

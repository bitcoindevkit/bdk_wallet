//! User facing wallet events.

use crate::collections::BTreeMap;
use crate::wallet::ChainPosition::{Confirmed, Unconfirmed};
use crate::Wallet;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitcoin::{Transaction, Txid};
use chain::{BlockId, ChainPosition, ConfirmationBlockTime};

/// Events representing changes to wallet transactions.
///
/// Returned after calling [`Wallet::apply_update`](crate::wallet::Wallet::apply_update).
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

pub(crate) fn wallet_events(
    wallet: &mut Wallet,
    chain_tip1: BlockId,
    chain_tip2: BlockId,
    wallet_txs1: BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>,
    wallet_txs2: BTreeMap<Txid, (Arc<Transaction>, ChainPosition<ConfirmationBlockTime>)>,
) -> Vec<WalletEvent> {
    let mut events: Vec<WalletEvent> = Vec::new();

    if chain_tip1 != chain_tip2 {
        events.push(WalletEvent::ChainTipChanged {
            old_tip: chain_tip1,
            new_tip: chain_tip2,
        });
    }

    wallet_txs2.iter().for_each(|(txid2, (tx2, cp2))| {
        if let Some((tx1, cp1)) = wallet_txs1.get(txid2) {
            assert_eq!(tx1.compute_txid(), *txid2);
            match (cp1, cp2) {
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
            match cp2 {
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

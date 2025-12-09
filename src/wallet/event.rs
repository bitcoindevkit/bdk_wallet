//! User facing wallet events.

use crate::collections::BTreeMap;
use crate::wallet::ChainPosition::{Confirmed, Unconfirmed};
use crate::Wallet;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitcoin::{Transaction, Txid};
use chain::{BlockId, ChainPosition, ConfirmationBlockTime};

/// Events representing changes to the wallet state.
///
/// Returned by [`Wallet::apply_update`], [`Wallet::apply_block`], and
/// [`Wallet::apply_block_connected_to`] to track transaction status changes and chain
/// tip changes due to new blocks or reorganizations.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WalletEvent {
    /// The blockchain tip known to the wallet has changed.
    ///
    /// Emitted when the blockchain is extended or a chain reorganization occurs.
    ChainTipChanged {
        /// The previous blockchain tip.
        old_tip: BlockId,
        /// The new blockchain tip.
        new_tip: BlockId,
    },

    /// A transaction has been confirmed in a block.
    ///
    /// Emitted when a transaction is first confirmed or re-confirmed in a different block after
    /// a chain reorganization. When `old_block_time` is `Some`, the transaction was previously
    /// confirmed in a different block.
    TxConfirmed {
        /// The transaction id.
        txid: Txid,
        /// The full transaction.
        tx: Arc<Transaction>,
        /// The block and timestamp where the transaction is confirmed.
        block_time: ConfirmationBlockTime,
        /// Previous confirmation details if re-confirmed after a reorg, `None` for first
        /// confirmation.
        old_block_time: Option<ConfirmationBlockTime>,
    },

    /// A transaction is now unconfirmed (in the mempool).
    ///
    /// Emitted when a transaction first appears in the mempool or when a confirmed transaction
    /// becomes unconfirmed due to a chain reorganization. When `old_block_time` is `Some`, the
    /// transaction was previously confirmed but is now unconfirmed due to a reorg.
    TxUnconfirmed {
        /// The transaction id.
        txid: Txid,
        /// The full transaction.
        tx: Arc<Transaction>,
        /// Previous confirmation details if unconfirmed due to reorg, `None` if first seen.
        old_block_time: Option<ConfirmationBlockTime>,
    },

    /// One or more unconfirmed transactions were replaced.
    ///
    /// Occurs when a transaction's inputs are spent by the replacement transaction, typically due
    /// to Replace-By-Fee (RBF) or a double-spend attempt.
    ///
    /// The `conflicts` field contains `(input_index, conflicting_txid)` pairs indicating which
    /// inputs conflict. Only conflicting transactions known to the wallet are reported.
    /// Conflicting transactions are usually added during a sync because they spend a UTXO tracked
    /// by the wallet.
    TxReplaced {
        /// The replacement transaction id.
        txid: Txid,
        /// The full replacement transaction.
        tx: Arc<Transaction>,
        /// List of `(input_index, conflicting_txid)` pairs showing which inputs were double-spent.
        conflicts: Vec<(usize, Txid)>,
    },

    /// An unconfirmed transaction was dropped from the mempool.
    ///
    /// This typically occurs when a transaction's fee rate is too low and/or the mempool is full.
    /// The transaction may reappear later if conditions change, which will emit a
    /// [`WalletEvent::TxUnconfirmed`] event.
    TxDropped {
        /// The dropped transaction id.
        txid: Txid,
        /// The full dropped transaction.
        tx: Arc<Transaction>,
    },
}

/// Generate events by comparing the chain tip and wallet transactions before and after applying
/// `wallet::Update` or a `bitcoin::Block` to `Wallet`. Any changes are added to the list of
/// returned `WalletEvent`s.
pub(crate) fn wallet_events(
    wallet: &mut Wallet,
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

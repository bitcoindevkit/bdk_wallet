//! Unbroadcasted transaction queue.

use alloc::vec::Vec;
use chain::tx_graph;
use chain::Anchor;
use chain::TxGraph;

use crate::collections::HashSet;
use crate::collections::VecDeque;

use bitcoin::Txid;
use chain::Merge;

/// An ordered unbroadcasted list.
///
/// It is ordered in case of RBF txs.
#[derive(Debug, Clone, Default)]
pub struct BroadcastQueue {
    queue: VecDeque<Txid>,

    /// Enforces that we do not have duplicates in `queue`.
    dedup: HashSet<Txid>,
}

/// Represents a single mutation to [`BroadcastQueue`].
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Mutation {
    /// A push to the back of the queue.
    Push(Txid),
    /// A removal from the queue.
    Remove(Txid),
}

/// A list of mutations made to [`BroadcastQueue`].
#[must_use]
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ChangeSet {
    /// Mutations.
    pub mutations: Vec<Mutation>,
}

impl Merge for ChangeSet {
    fn merge(&mut self, other: Self) {
        self.mutations.extend(other.mutations);
    }

    fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }
}

impl BroadcastQueue {
    /// Construct [`Unbroadcasted`] from the given `changeset`.
    pub fn from_changeset(changeset: ChangeSet) -> Self {
        let mut out = BroadcastQueue::default();
        out.apply_changeset(changeset);
        out
    }

    /// Apply the given `changeset`.
    pub fn apply_changeset(&mut self, changeset: ChangeSet) {
        for mutation in changeset.mutations {
            match mutation {
                Mutation::Push(txid) => self._push(txid),
                Mutation::Remove(txid) => self._remove(txid),
            };
        }
    }

    /// Whether the `txid` exists in the queue.
    pub fn contains(&self, txid: Txid) -> bool {
        self.dedup.contains(&txid)
    }

    /// Push a `txid` to the queue if it does not already exist.
    ///
    /// # Warning
    ///
    /// This does not get rid of conflicting transactions already in the queue.
    pub fn push(&mut self, txid: Txid) -> ChangeSet {
        let mut changeset = ChangeSet::default();
        if self._push(txid) {
            changeset.mutations.push(Mutation::Push(txid));
        }
        changeset
    }
    fn _push(&mut self, txid: Txid) -> bool {
        if self.dedup.insert(txid) {
            self.queue.push_back(txid);
            return true;
        }
        false
    }

    /// Push a `txid` to the broadcast queue (if it does not exist already) and displaces all
    /// coflicting txids in the queue.
    pub fn push_and_displace_conflicts<A>(&mut self, tx_graph: &TxGraph<A>, txid: Txid) -> ChangeSet
    where
        A: Anchor,
    {
        let mut changeset = ChangeSet::default();

        let tx = match tx_graph.get_tx(txid) {
            Some(tx) => tx,
            None => {
                debug_assert!(
                    !self.dedup.contains(&txid),
                    "Cannot have txid in queue which has no corresponding tx in graph"
                );
                return changeset;
            }
        };

        if self._push(txid) {
            changeset.mutations.push(Mutation::Push(txid));

            for txid in tx_graph.walk_conflicts(&tx, |_, conflict_txid| Some(conflict_txid)) {
                if self._remove(txid) {
                    changeset.mutations.push(Mutation::Remove(txid));
                }
            }
        }

        changeset
    }

    /// Returns the next `txid` of the queue to broadcast which has no dependencies to other
    /// transactions in the queue.
    pub fn next_to_broadcast<A>(&self, tx_graph: &TxGraph<A>) -> Option<Txid>
    where
        A: Anchor,
    {
        self.queue.iter().copied().find(|&txid| {
            let tx = match tx_graph.get_tx(txid) {
                Some(tx) => tx,
                None => return false,
            };
            if tx
                .input
                .iter()
                .any(|txin| self.dedup.contains(&txin.previous_output.txid))
            {
                return false;
            }
            true
        })
    }

    /// Returns unbroadcasted dependencies of the given `txid`.
    ///
    /// The returned `Vec` is in broadcast order.
    pub fn unbroadcasted_dependencies<A>(&self, tx_graph: &TxGraph<A>, txid: Txid) -> Vec<Txid>
    where
        A: Anchor,
    {
        let tx = match tx_graph.get_tx(txid) {
            Some(tx) => tx,
            None => return Vec::new(),
        };
        let mut txs = tx_graph
            .walk_ancestors(tx, |_depth, ancestor_tx| {
                let ancestor_txid = ancestor_tx.compute_txid();
                if self.dedup.contains(&ancestor_txid) {
                    Some(ancestor_txid)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        txs.reverse();
        txs
    }

    /// Untracks and removes a transaction from the broadcast queue.
    ///
    /// Transactions are automatically removed from the queue upon successful broadcast, so calling
    /// this method directly is typically not required.
    pub fn remove(&mut self, txid: Txid) -> ChangeSet {
        let mut changeset = ChangeSet::default();
        if self._remove(txid) {
            changeset.mutations.push(Mutation::Remove(txid));
        }
        changeset
    }
    fn _remove(&mut self, txid: Txid) -> bool {
        if self.dedup.remove(&txid) {
            let i = (0..self.queue.len())
                .zip(self.queue.iter().copied())
                .find_map(|(i, queue_txid)| if queue_txid == txid { Some(i) } else { None })
                .expect("must exist in queue to exist in `queue`");
            let _removed = self.queue.remove(i);
            debug_assert_eq!(_removed, Some(txid));
            return true;
        }
        false
    }

    /// Untracks and removes a transaction and it's descendants from the broadcast queue.
    pub fn remove_and_displace_dependants<A>(
        &mut self,
        tx_graph: &TxGraph<A>,
        txid: Txid,
    ) -> ChangeSet
    where
        A: Anchor,
    {
        let mut changeset = ChangeSet::default();

        if self._remove(txid) {
            changeset.mutations.push(Mutation::Remove(txid));
            for txid in tx_graph.walk_descendants(txid, |_depth, txid| Some(txid)) {
                if self._remove(txid) {
                    changeset.mutations.push(Mutation::Remove(txid));
                }
            }
        }
        changeset
    }

    /// Untrack transactions that are given anchors and/or mempool timestamps.
    pub fn filter_from_graph_changeset<A>(
        &mut self,
        graph_changeset: &tx_graph::ChangeSet<A>,
    ) -> ChangeSet {
        let mut changeset = ChangeSet::default();
        let s_txids = graph_changeset.last_seen.keys().copied();
        let a_txids = graph_changeset.anchors.iter().map(|(_, txid)| *txid);
        let e_txids = graph_changeset.last_evicted.keys().copied();
        for txid in s_txids.chain(a_txids).chain(e_txids) {
            changeset.merge(self.remove(txid));
        }
        changeset
    }

    /// Txids ordered by precedence.
    ///
    /// Transactions with greater precedence will appear later in this list.
    pub fn txids(&self) -> impl ExactSizeIterator<Item = Txid> + '_ {
        self.queue.iter().copied()
    }

    /// Initial changeset.
    pub fn initial_changeset(&self) -> ChangeSet {
        ChangeSet {
            mutations: self.queue.iter().copied().map(Mutation::Push).collect(),
        }
    }
}

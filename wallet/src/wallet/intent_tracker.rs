//! Unbroadcasted transaction queue.

use alloc::sync::Arc;

use alloc::vec::Vec;
use bitcoin::OutPoint;
use bitcoin::Transaction;
use chain::tx_graph;
use chain::Anchor;
use chain::CanonicalIter;
use chain::CanonicalReason;
use chain::ChainOracle;
use chain::ChainPosition;
use chain::TxGraph;

use crate::collections::BTreeMap;
use crate::collections::HashMap;
use crate::collections::HashSet;
use crate::collections::VecDeque;

use bdk_chain::bdk_core::Merge;
use bitcoin::Txid;

#[derive(Debug)]
pub struct CanonicalView<A> {
    pub txs: HashMap<Txid, (Arc<Transaction>, CanonicalReason<A>)>,
    pub spends: HashMap<OutPoint, Txid>,
}

impl<A> Default for CanonicalView<A> {
    fn default() -> Self {
        Self {
            txs: HashMap::new(),
            spends: HashMap::new(),
        }
    }
}

impl<A> CanonicalView<A> {
    pub fn from_iter<C>(iter: CanonicalIter<'_, A, C>) -> Result<Self, C::Error>
    where
        A: Anchor,
        C: ChainOracle,
    {
        let mut view = Self::default();
        for r in iter {
            let (txid, tx, reason) = r?;
            for txin in &tx.input {
                view.spends.insert(txin.previous_output, txid);
            }
            view.txs.insert(txid, (tx, reason));
        }
        Ok(view)
    }

    pub fn spend(&self, op: OutPoint) -> Option<(Txid, Arc<Transaction>, &CanonicalReason<A>)> {
        let txid = self.spends.get(&op)?;
        let (tx, reason) = self.txs.get(txid)?;
        Some((*txid, tx.clone(), reason))
    }
}

/// Indicates whether a transaction was observed in the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkSeen {
    /// The transaction was previously seen (e.g., in mempool or on-chain).
    Seen,
    /// The transaction was never seen in the network.
    NeverSeen,
}

impl NetworkSeen {
    /// Whether the transaction was once observed in the network.
    pub fn was_seen(self) -> bool {
        match self {
            NetworkSeen::Seen => true,
            NetworkSeen::NeverSeen => false,
        }
    }
}

/// Represents an input (spend) that depends on non-canonical transaction ancestry.
///
/// This struct models an input that attempts to spend an output via a transaction path
/// that is not part of the canonical network view (e.g., evicted, conflicted, or unknown).
#[derive(Debug, Clone, Default)]
pub struct UncanonicalSpendInfo<A> {
    /// Non-canonical ancestor transactions reachable from this input.
    ///
    /// Each entry maps an ancestor `Txid` to its observed status in the network.
    /// - `Seen` indicates the transaction was previously seen but is no longer part of the
    ///   canonical view.
    /// - `NeverSeen` indicates it was never observed (e.g., not yet broadcast).
    pub uncanonical_ancestors: BTreeMap<Txid, NetworkSeen>,

    /// Canonical transactions that conflict with this spend.
    ///
    /// This may be a direct conflict or a conflict with one of the `uncanonical_ancestors`.
    /// The value is a tuple of (conflict distance, chain position).   
    ///
    /// Descendants of conflicts are also conflicts. These transactions will have the same distance
    /// value as their conflicting parent.
    pub conflicting_txs: BTreeMap<Txid, (u32, ChainPosition<A>)>,
}

/// Tracked and uncanonical transaction.
#[derive(Debug, Clone)]
pub struct UncanonicalTx<A> {
    /// Txid.
    pub txid: Txid,
    /// The uncanonical transaction.
    pub tx: Arc<Transaction>,
    /// Whether the transaction was one seen by the network.
    pub network_seen: NetworkSeen,
    /// Spends, identified by prevout, which are uncanonical.
    pub uncanonical_spends: BTreeMap<OutPoint, UncanonicalSpendInfo<A>>,
}

impl<A: Anchor> UncanonicalTx<A> {
    /// Whether the transaction was once observed in the network.
    ///
    /// Assuming that the chain-source does not lie, we can safely remove transactions that
    pub fn was_seen(&self) -> bool {
        self.network_seen.was_seen()
    }

    /// A transaction is safe to untrack if it is network uncanonical and we can gurarantee that
    /// it will not become canonical again given that there is no reorg of depth greater than
    /// `assume_final_depth`.
    ///
    /// `assume_final_depth` of `0` means that unconfirmed (mempool) transactions are assumed to be
    /// final.
    ///
    /// This may return false-negatives if the wallet is unaware of conflicts. I.e. if purely
    /// syncing with Electrum (TODO: @evanlinjin Expand on this).
    pub fn is_safe_to_untrack(&self, tip_height: u32, assume_final_depth: u32) -> bool {
        self.conflicts().any(|(_, pos)| {
            let depth = match pos {
                ChainPosition::Confirmed { anchor, .. } => {
                    tip_height.saturating_sub(anchor.confirmation_height_upper_bound())
                }
                ChainPosition::Unconfirmed { .. } => 0,
            };
            depth >= assume_final_depth
        })
    }

    /// Iterate over transactions that are currently canonical in the network, but would be rendered
    /// uncanonical if this transaction were to become canonical.
    ///
    /// This includes both direct and indirect conflicts, such as any transaction that relies on
    /// conflicting ancestry.
    pub fn conflicts(&self) -> impl Iterator<Item = (Txid, &ChainPosition<A>)> {
        self.uncanonical_spends
            .values()
            .flat_map(|spend| &spend.conflicting_txs)
            .map(|(&txid, (_, pos))| (txid, pos))
            .filter({
                let mut dedup = HashSet::<Txid>::new();
                move |(txid, _)| dedup.insert(*txid)
            })
    }

    pub fn confirmed_conflicts(&self) -> impl Iterator<Item = (Txid, &A)> {
        self.conflicts().filter_map(|(txid, pos)| match pos {
            ChainPosition::Confirmed { anchor, .. } => Some((txid, anchor)),
            ChainPosition::Unconfirmed { .. } => None,
        })
    }

    pub fn unconfirmed_conflicts(&self) -> impl Iterator<Item = Txid> + '_ {
        self.conflicts().filter_map(|(txid, pos)| match pos {
            ChainPosition::Confirmed { .. } => None,
            ChainPosition::Unconfirmed { .. } => Some(txid),
        })
    }

    /// Missing ancestors.
    ///
    /// Either evicted from mempool, or never successfully broadcast in the first place.
    pub fn missing_parents(&self) -> impl Iterator<Item = (Txid, NetworkSeen)> + '_ {
        self.uncanonical_spends
            .values()
            .flat_map(|spend_info| &spend_info.uncanonical_ancestors)
            .map(|(&txid, &network_seen)| (txid, network_seen))
    }

    pub fn contains_conflicts(&self) -> bool {
        self.conflicts().next().is_some()
    }

    pub fn contains_confirmed_conflicts(&self) -> bool {
        self.confirmed_conflicts().next().is_some()
    }
}

/// An ordered unbroadcasted staging area.
///
/// It is ordered in case of RBF txs.
#[derive(Debug, Clone, Default)]
pub struct CanonicalizationTracker {
    /// Tracks the order that transactions are added.
    order: VecDeque<Txid>,

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

impl CanonicalizationTracker {
    /// Construct [`Unbroadcasted`] from the given `changeset`.
    pub fn from_changeset(changeset: ChangeSet) -> Self {
        let mut out = CanonicalizationTracker::default();
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
            self.order.push_back(txid);
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
        self.order.iter().copied().find(|&txid| {
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
            let i = (0..self.order.len())
                .zip(self.order.iter().copied())
                .find_map(|(i, queue_txid)| if queue_txid == txid { Some(i) } else { None })
                .expect("must exist in queue to exist in `queue`");
            let _removed = self.order.remove(i);
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
        self.order.iter().copied()
    }

    /// Initial changeset.
    pub fn initial_changeset(&self) -> ChangeSet {
        ChangeSet {
            mutations: self.order.iter().copied().map(Mutation::Push).collect(),
        }
    }
}

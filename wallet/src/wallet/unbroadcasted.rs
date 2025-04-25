//! Unbroadcasted transaction queue.

use crate::collections::BTreeMap;

use crate::collections::HashSet;

use crate::collections::hash_map;
use crate::collections::HashMap;

use bitcoin::Txid;
use chain::Merge;
use chain::TxUpdate;

/// An ordered unbroadcasted list.
///
/// It is ordered in case of RBF txs.
#[derive(Debug, Clone, Default)]
pub struct Unbroadcasted {
    txs: HashMap<Txid, u64>,
    order: HashSet<(u64, Txid)>,
    next_seq: u64,
}

/// Represents changes made to [`Unbroadcasted`].
#[must_use]
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ChangeSet {
    /// Add or remove?
    pub txs: BTreeMap<(u64, Txid), bool>,
}

impl Merge for ChangeSet {
    fn merge(&mut self, other: Self) {
        self.txs.merge(other.txs);
    }

    fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }
}

impl Unbroadcasted {
    /// Construct [`Unbroadcasted`] from the given `changeset`.
    pub fn from_changeset(changeset: ChangeSet) -> Self {
        let mut out = Unbroadcasted::default();
        out.apply_changeset(changeset);
        out
    }

    /// Apply the given `changeset`.
    pub fn apply_changeset(&mut self, changeset: ChangeSet) {
        for ((_, txid), is_add) in changeset.txs {
            if is_add {
                let _ = self.insert(txid);
            } else {
                let _ = self.remove(txid);
            }
        }
    }

    /// Reinserting will bump the tx's seq to `next_seq`.
    pub fn insert(&mut self, txid: Txid) -> ChangeSet {
        let seq = self.next_seq;
        self.next_seq += 1;

        match self.txs.entry(txid) {
            hash_map::Entry::Occupied(mut entry) => {
                // remove stuff
                let entry_seq = entry.get_mut();
                self.order.remove(&(*entry_seq, txid));
                self.order.insert((seq, txid));
                *entry_seq = seq;
            }
            hash_map::Entry::Vacant(entry) => {
                entry.insert(seq);
                self.order.insert((seq, txid));
            }
        }

        let mut changeset = ChangeSet::default();
        changeset.txs.insert((seq, txid), true);
        changeset
    }

    /// Untrack the `txid`.
    pub fn remove(&mut self, txid: Txid) -> ChangeSet {
        let mut changeset = ChangeSet::default();
        if let Some(seq) = self.txs.remove(&txid) {
            self.order.remove(&(seq, txid));

            let seq = self.next_seq;
            self.next_seq += 1;

            changeset.txs.insert((seq, txid), false);
        }
        changeset
    }

    /// Untrack transactions that are given anchors and seen-at timestamps.
    pub fn update<A>(&mut self, tx_update: &TxUpdate<A>) -> ChangeSet {
        let mut changeset = ChangeSet::default();
        for (_, txid) in &tx_update.anchors {
            changeset.merge(self.remove(*txid));
        }
        for (txid, _) in &tx_update.seen_ats {
            changeset.merge(self.remove(*txid));
        }
        changeset
    }

    /// Txids ordered by precedence.
    ///
    /// Transactions with greater precedence will appear later in this list.
    pub fn txids(&self) -> impl ExactSizeIterator<Item = Txid> + '_ {
        self.order.iter().map(|&(_, txid)| txid)
    }
}

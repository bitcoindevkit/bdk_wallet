//! Module containing the locked outpoints change set.

use bdk_chain::Merge;
use bitcoin::OutPoint;
use serde::{Deserialize, Serialize};

use crate::collections::BTreeMap;

/// Represents changes to locked outpoints.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChangeSet {
    /// The lock status of an outpoint, `true == is_locked`.
    pub outpoints: BTreeMap<OutPoint, bool>,
}

impl Merge for ChangeSet {
    fn merge(&mut self, other: Self) {
        // Extend self with other. Any entries in `self` that share the same
        // outpoint are overwritten.
        self.outpoints.extend(other.outpoints);
    }

    fn is_empty(&self) -> bool {
        self.outpoints.is_empty()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bdk_chain::Merge;
    use bitcoin::{hashes::Hash, OutPoint, Txid};

    /// Helper to create an `OutPoint` from an index byte.
    fn outpoint(vout: u32) -> OutPoint {
        OutPoint {
            txid: Txid::from_byte_array([vout as u8; 32]),
            vout,
        }
    }

    #[test]
    fn test_is_empty_default() {
        let cs = ChangeSet::default();
        assert!(cs.is_empty());
    }

    #[test]
    fn test_is_empty_with_entries() {
        let cs = ChangeSet {
            outpoints: [(outpoint(0), true)].into(),
        };
        assert!(!cs.is_empty());
    }

    #[test]
    fn test_merge_into_empty() {
        let mut cs = ChangeSet::default();
        let other = ChangeSet {
            outpoints: [(outpoint(1), true)].into(),
        };
        cs.merge(other);
        assert_eq!(cs.outpoints.len(), 1);
        assert!(cs.outpoints[&outpoint(1)]);
    }

    #[test]
    fn test_merge_empty_into_non_empty() {
        let mut cs = ChangeSet {
            outpoints: [(outpoint(0), true)].into(),
        };
        let snapshot = cs.clone();
        cs.merge(ChangeSet::default());
        assert_eq!(cs, snapshot);
    }

    #[test]
    fn test_merge_disjoint() {
        let mut cs = ChangeSet {
            outpoints: [(outpoint(0), true)].into(),
        };
        let other = ChangeSet {
            outpoints: [(outpoint(1), false)].into(),
        };
        cs.merge(other);
        assert_eq!(cs.outpoints.len(), 2);
        assert!(cs.outpoints[&outpoint(0)]);
        assert!(!cs.outpoints[&outpoint(1)]);
    }

    #[test]
    fn test_merge_overwrites_duplicate() {
        let op = outpoint(0);
        let mut cs = ChangeSet {
            outpoints: [(op, true)].into(),
        };
        let other = ChangeSet {
            outpoints: [(op, false)].into(),
        };
        cs.merge(other);
        assert_eq!(cs.outpoints.len(), 1);
        assert!(!cs.outpoints[&op], "other's value should overwrite self");
    }
}

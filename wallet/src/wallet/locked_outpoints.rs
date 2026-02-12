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

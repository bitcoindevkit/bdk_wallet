//! Module containing the locked outpoints change set.

use bdk_chain::Merge;
use bitcoin::OutPoint;
use serde::{Deserialize, Serialize};

use crate::collections::BTreeMap;

/// Represents changes to locked outpoints.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChangeSet {
    /// The lock status of an outpoint, `true == is_locked`.
    pub locked_outpoints: BTreeMap<OutPoint, bool>,
}

impl Merge for ChangeSet {
    fn merge(&mut self, other: Self) {
        // Extend self with other. Any entries in `self` that share the same
        // outpoint are overwritten.
        self.locked_outpoints.extend(other.locked_outpoints);
    }

    fn is_empty(&self) -> bool {
        self.locked_outpoints.is_empty()
    }
}

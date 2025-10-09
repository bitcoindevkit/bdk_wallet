//! Parameters for PSBT building.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use bdk_chain::{BlockId, CanonicalizationParams, ConfirmationBlockTime, FullTxOut, TxGraph};
use bdk_tx::{DefiniteDescriptor, Input, Output};
use bitcoin::{
    absolute, transaction::Version, Amount, FeeRate, OutPoint, ScriptBuf, Sequence, Transaction,
    Txid,
};
use miniscript::plan::Assets;

use crate::collections::HashSet;
use crate::TxOrdering;

/// Parameters to create a PSBT.
#[derive(Debug)]
pub struct PsbtParams {
    // Inputs
    pub(crate) set: HashSet<OutPoint>,
    pub(crate) utxos: Vec<OutPoint>,
    pub(crate) inputs: Vec<Input>,

    // Outputs
    pub(crate) recipients: Vec<(ScriptBuf, Amount)>,
    pub(crate) change_descriptor: Option<DefiniteDescriptor>,

    // Coin Selection
    pub(crate) assets: Option<Assets>,
    pub(crate) feerate: FeeRate,
    pub(crate) longterm_feerate: FeeRate,
    pub(crate) drain_wallet: bool,
    pub(crate) coin_selection: SelectionStrategy,
    pub(crate) canonical_params: CanonicalizationParams,
    pub(crate) utxo_filter: UtxoFilter,

    // PSBT
    pub(crate) version: Option<Version>,
    pub(crate) locktime: Option<absolute::LockTime>,
    pub(crate) fallback_sequence: Option<Sequence>,
    pub(crate) ordering: TxOrdering<Input, Output>,
}

impl Default for PsbtParams {
    fn default() -> Self {
        Self {
            set: Default::default(),
            utxos: Default::default(),
            inputs: Default::default(),
            assets: Default::default(),
            recipients: Default::default(),
            change_descriptor: Default::default(),
            feerate: bitcoin::FeeRate::BROADCAST_MIN,
            longterm_feerate: bitcoin::FeeRate::from_sat_per_vb_unchecked(10),
            drain_wallet: Default::default(),
            coin_selection: Default::default(),
            canonical_params: Default::default(),
            utxo_filter: Default::default(),
            version: Default::default(),
            locktime: Default::default(),
            fallback_sequence: Default::default(),
            ordering: Default::default(),
        }
    }
}

impl PsbtParams {
    /// Add UTXOs by outpoint to fund the transaction.
    ///
    /// A single outpoint may appear at most once in the list of UTXOs to spend. The caller is
    /// responsible for ensuring that elements of `outpoints` correspond to outputs of previous
    /// transactions and are currently unspent.
    pub fn add_utxos(&mut self, outpoints: &[OutPoint]) -> &mut Self {
        self.utxos
            .extend(outpoints.iter().copied().filter(|&op| self.set.insert(op)));
        self
    }

    /// Get the currently selected spends.
    pub fn utxos(&self) -> &HashSet<OutPoint> {
        &self.set
    }

    /// Remove a UTXO from the currently selected inputs.
    pub fn remove_utxo(&mut self, outpoint: &OutPoint) -> &mut Self {
        if self.set.remove(outpoint) {
            self.utxos.retain(|op| op != outpoint);
        }
        self
    }

    /// Add the spend [`Assets`].
    ///
    /// Assets are required to create a spending plan for an output controlled by the wallet's
    /// descriptors. If none are provided here, then we assume all of the keys are equally likely
    /// to sign.
    ///
    /// This may be called multiple times to add additional assets, however only the last
    /// absolute or relative timelock is retained. See also `AssetsExt`.
    pub fn add_assets(&mut self, assets: Assets) -> &mut Self {
        let mut new = match self.assets {
            Some(ref existing) => {
                let mut new = Assets::new();
                new.extend(existing);
                new
            }
            None => Assets::new(),
        };
        new.extend(&assets);
        self.assets = Some(new);
        self
    }

    /// Add recipients.
    ///
    /// - `recipients`: An iterator of `(S, Amount)` tuples where `S` can be a bitcoin [`Address`],
    ///   a scriptPubKey, or anything that can be converted straight into a [`ScriptBuf`].
    pub fn add_recipients<I, S>(&mut self, recipients: I) -> &mut Self
    where
        I: IntoIterator<Item = (S, Amount)>,
        S: Into<ScriptBuf>,
    {
        self.recipients
            .extend(recipients.into_iter().map(|(s, amt)| (s.into(), amt)));
        self
    }

    /// Set the target fee rate.
    pub fn feerate(&mut self, feerate: FeeRate) -> &mut Self {
        self.feerate = feerate;
        self
    }

    /// Set the strategy to be used when selecting coins.
    pub fn coin_selection(&mut self, strategy: SelectionStrategy) -> &mut Self {
        self.coin_selection = strategy;
        self
    }

    /// Set the definite descriptor used for generating the change output.
    pub fn change_descriptor(&mut self, desc: DefiniteDescriptor) -> &mut Self {
        self.change_descriptor = Some(desc);
        self
    }

    /// Replace spends of the given `txs` and return a [`ReplaceParams`] populated with the
    /// the inputs to spend.
    ///
    /// This merges all of the spends into a single transaction while retaining the parameters
    /// of `self`. Note however that any previously added UTXOs are removed. Call
    /// [`replace_by_fee_with_aux_rand`](crate::Wallet::replace_by_fee_with_aux_rand) to finish
    /// building the PSBT.
    ///
    /// ## Note
    ///
    /// There should be no ancestry linking the elements of `txs`, since replacing an
    /// ancestor necessarily invalidates the descendant.
    pub fn replace(self, txs: &[Arc<Transaction>]) -> ReplaceParams {
        ReplaceParams::new(txs, self)
    }

    /// Filter [`FullTxOut`]s by the provided closure.
    ///
    /// This option can be used to mark specific outputs unspendable or apply any sort of custom
    /// UTXO filter.
    ///
    /// Note that returning `true` from the `exclude` function will exclude the output from coin
    /// selection, otherwise any coin in the wallet that is mature and spendable will be
    /// eligible for selection.
    pub fn filter_utxos<F>(&mut self, exclude: F) -> &mut Self
    where
        F: Fn(&FullTxOut<ConfirmationBlockTime>) -> bool + Send + Sync + 'static,
    {
        self.utxo_filter = UtxoFilter(Arc::new(exclude));
        self
    }

    /// Set the [`TxOrdering`] for inputs and outputs of the PSBT.
    ///
    /// If not set here, the default ordering is to [`Shuffle`] all inputs and outputs.
    ///
    /// Set to [`Untouched`] to preserve the order of UTXOs and recipients in the manner in which
    /// they are added to the params. If additional inputs are required that aren't manually
    /// selected, their order will be determined by the [`SelectionStrategy`]. Refer to
    /// [`TxOrdering`] for more.
    ///
    /// [`Shuffle`]: TxOrdering::Shuffle
    /// [`Untouched`]: TxOrdering::Untouched
    pub fn ordering(&mut self, ordering: TxOrdering<Input, Output>) -> &mut Self {
        self.ordering = ordering;
        self
    }

    /// Add a planned input.
    ///
    /// This can be used to add inputs that come with a [`Plan`] or [`psbt::Input`] provided.
    ///
    /// [`Plan`]: miniscript::plan::Plan
    /// [`psbt::Input`]: bitcoin::psbt::Input
    pub fn add_planned_input(&mut self, input: Input) -> &mut Self {
        if self.set.insert(input.prev_outpoint()) {
            self.inputs.push(input);
        }
        self
    }
}

/// Coin select strategy.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub enum SelectionStrategy {
    /// Single random draw.
    #[default]
    SingleRandomDraw,
    /// Lowest fee, a variation of Branch 'n Bound that allows for change
    /// while minimizing transaction fees. Refer to
    /// [`LowestFee`](bdk_coin_select::metrics::LowestFee) metric for more.
    LowestFee,
}

/// [`UtxoFilter`] is a user-defined `Fn` closure which decides whether to exclude a UTXO
/// from being selected.
// TODO: Consider having this also take a `&Wallet` in case the caller needs information
// not given by the FullTxOut.
#[allow(clippy::type_complexity)]
pub(crate) struct UtxoFilter(
    pub Arc<dyn Fn(&FullTxOut<ConfirmationBlockTime>) -> bool + Send + Sync>,
);

impl Default for UtxoFilter {
    fn default() -> Self {
        Self(Arc::new(|_| false))
    }
}

impl fmt::Debug for UtxoFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UtxoFilter")
    }
}

/// `ReplaceParams` provides a thin wrapper around [`PsbtParams`] and is intended for
/// crafting Replace-By-Fee transactions (RBF).
#[derive(Debug, Default)]
pub struct ReplaceParams {
    /// Txids of txs to replace.
    pub(crate) replace: HashSet<Txid>,
    /// The inner PSBT parameters.
    pub(crate) inner: PsbtParams,
}

impl ReplaceParams {
    /// Construct from `inner` params and the `txs` to replace.
    pub(crate) fn new(txs: &[Arc<Transaction>], inner: PsbtParams) -> Self {
        let mut params = Self {
            inner,
            ..Default::default()
        };
        params.replace(txs);
        params
    }

    /// Replace spends of the provided `txs`. This will internally set the internal list
    /// of UTXOs to be spent.
    pub fn replace(&mut self, txs: &[Arc<Transaction>]) {
        self.inner.utxos.clear();
        let mut utxos = vec![];

        let (mut txids_to_replace, txs): (HashSet<Txid>, Vec<Transaction>) = txs
            .iter()
            .map(|tx| (tx.compute_txid(), tx.as_ref().clone()))
            .unzip();
        let tx_graph = TxGraph::<BlockId>::new(txs);

        // Sanitize the RBF set by removing elements of `txs` which have ancestors
        // in the same set. This is to avoid spending outputs of txs that are bound
        // for replacement.
        for tx_node in tx_graph.full_txs() {
            let tx = &tx_node.tx;
            if tx.is_coinbase()
                || tx_graph
                    .walk_ancestors(Arc::clone(tx), |_, tx| Some(tx.compute_txid()))
                    .any(|ancestor_txid| txids_to_replace.contains(&ancestor_txid))
            {
                txids_to_replace.remove(&tx_node.txid);
            } else {
                utxos.extend(tx.input.iter().map(|txin| txin.previous_output));
            }
        }

        self.replace = txids_to_replace;
        self.inner.add_utxos(&utxos);
    }

    /// Add recipients.
    pub fn add_recipients<I, S>(&mut self, recipients: I) -> &mut Self
    where
        I: IntoIterator<Item = (S, Amount)>,
        S: Into<ScriptBuf>,
    {
        self.inner.add_recipients(recipients);
        self
    }

    /// Set the target fee rate.
    pub fn feerate(&mut self, feerate: FeeRate) -> &mut Self {
        self.inner.feerate(feerate);
        self
    }

    /// Get the currently selected spends.
    pub fn utxos(&self) -> &HashSet<OutPoint> {
        &self.inner.set
    }

    /// Remove a UTXO from the currently selected inputs.
    pub fn remove_utxo(&mut self, outpoint: &OutPoint) -> &mut Self {
        self.inner.remove_utxo(outpoint);
        self
    }
}

/// Trait to extend the functionality of [`Assets`].
pub(crate) trait AssetsExt {
    /// Extend `self` with the contents of `other`.
    fn extend(&mut self, other: &Self);
}

impl AssetsExt for Assets {
    /// Extend `self` with the contents of `other`. Note that if present this preferentially
    /// uses the absolute and relative timelocks of `other`.
    fn extend(&mut self, other: &Self) {
        self.keys.extend(other.keys.clone());
        self.sha256_preimages.extend(other.sha256_preimages.clone());
        self.hash256_preimages
            .extend(other.hash256_preimages.clone());
        self.ripemd160_preimages
            .extend(other.ripemd160_preimages.clone());
        self.hash160_preimages
            .extend(other.hash160_preimages.clone());

        self.absolute_timelock = other.absolute_timelock.or(self.absolute_timelock);
        self.relative_timelock = other.relative_timelock.or(self.relative_timelock);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::new_tx;

    use bitcoin::hashes::Hash;
    use bitcoin::{TxIn, TxOut};

    #[test]
    fn test_sanitize_rbf_set() {
        // To replace the set { [A, B], [C] }, where B is a descendant of A:
        // We shouldn't try to replace the inputs of B, because replacing A will render A's outputs
        // unspendable. Therefore the RBF inputs should only contain the inputs of A and C.

        // A is an ancestor
        let tx_a = Transaction {
            input: vec![TxIn {
                previous_output: OutPoint::new(Hash::hash(b"parent_a"), 0),
                ..Default::default()
            }],
            output: vec![TxOut::NULL],
            ..new_tx(0)
        };
        let txid_a = tx_a.compute_txid();
        // B spends A
        let tx_b = Transaction {
            input: vec![TxIn {
                previous_output: OutPoint::new(txid_a, 0),
                ..Default::default()
            }],
            output: vec![TxOut::NULL],
            ..new_tx(1)
        };
        // C is an ancestor
        let tx_c = Transaction {
            input: vec![TxIn {
                previous_output: OutPoint::new(Hash::hash(b"parent_c"), 0),
                ..Default::default()
            }],
            output: vec![TxOut::NULL],
            ..new_tx(2)
        };
        let txid_c = tx_c.compute_txid();
        // D is unrelated coinbase tx
        let tx_d = Transaction {
            input: vec![TxIn::default()],
            output: vec![TxOut::NULL],
            ..new_tx(3)
        };

        let expect_spends: HashSet<OutPoint> =
            [tx_a.input[0].previous_output, tx_c.input[0].previous_output].into();

        let txs: Vec<Arc<Transaction>> =
            [tx_a, tx_b, tx_c, tx_d].into_iter().map(Arc::new).collect();
        let params = ReplaceParams::new(&txs, PsbtParams::default());
        assert_eq!(params.inner.set, expect_spends);
        assert_eq!(params.replace, [txid_a, txid_c].into());
    }

    #[test]
    fn test_selected_outpoints_are_unique() {
        let mut params = PsbtParams::default();
        let op = OutPoint::null();

        // Try adding the same outpoint repeatedly.
        for _ in 0..3 {
            params.add_utxos(&[op]);
        }
        assert_eq!(
            params.utxos(),
            &[op].into(),
            "Failed to filter duplicate outpoints"
        );
        assert_eq!(params.set, [op].into());

        params.utxos = vec![];

        // Try adding duplicates in the same set.
        params.add_utxos(&[op, op, op]);
        assert_eq!(
            params.utxos(),
            &[op].into(),
            "Failed to filter duplicate outpoints"
        );
        assert_eq!(params.set, [op].into());
    }
}

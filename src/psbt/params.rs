//! Parameters for creating a PSBT.

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

/// Marker type representing the PSBT creation state.
#[derive(Debug)]
pub struct CreateTx;

/// Marker type representing the Replace-By-Fee (RBF) state.
#[derive(Debug)]
pub struct ReplaceTx;

/// Alias for [`ReplaceTx`] context marker.
pub type Rbf = ReplaceTx;

/// Parameters to create a PSBT.
#[derive(Debug)]
pub struct PsbtParams<C> {
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
    pub(crate) maturity_height: Option<u32>,
    pub(crate) manually_selected_only: bool,

    // PSBT
    pub(crate) version: Option<Version>,
    pub(crate) locktime: Option<absolute::LockTime>,
    pub(crate) fallback_sequence: Option<Sequence>,
    pub(crate) ordering: TxOrdering<Input, Output>,
    pub(crate) only_witness_utxo: bool,
    pub(crate) add_global_xpubs: bool,

    // RBF
    pub(crate) replace: HashSet<Txid>,

    /// Context marker.
    pub(crate) marker: core::marker::PhantomData<C>,
}

impl Default for PsbtParams<CreateTx> {
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
            maturity_height: Default::default(),
            manually_selected_only: Default::default(),
            version: Default::default(),
            locktime: Default::default(),
            fallback_sequence: Default::default(),
            ordering: Default::default(),
            only_witness_utxo: Default::default(),
            add_global_xpubs: Default::default(),
            replace: Default::default(),
            marker: core::marker::PhantomData,
        }
    }
}

impl PsbtParams<CreateTx> {
    /// Create a new [`PsbtParams`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Add UTXOs by outpoint to fund the transaction.
    ///
    /// A single outpoint may appear at most once in the list of UTXOs to spend. The caller is
    /// responsible for ensuring that items of `outpoints` correspond to outputs of previous
    /// transactions and are currently unspent.
    ///
    /// If an outpoint doesn't correspond to an indexed script pubkey, a [`UnknownUtxo`]
    /// error will occur. See [`Wallet::create_psbt`] for more.
    ///
    /// To add a UTXO that did not originate from this wallet (i.e. a "foreign" UTXO), see
    /// [`PsbtParams::add_planned_input`].
    ///
    /// [`UnknownUtxo`]: crate::wallet::error::CreatePsbtError::UnknownUtxo
    /// [`Wallet::create_psbt`]: crate::Wallet::create_psbt
    pub fn add_utxos(&mut self, outpoints: &[OutPoint]) -> &mut Self {
        self.utxos
            .extend(outpoints.iter().copied().filter(|&op| self.set.insert(op)));
        self
    }

    /// Replace spends of the given `txs` and return a [`PsbtParams`] populated with the
    /// the inputs to spend.
    ///
    /// This merges all of the spends into a single transaction while retaining the parameters
    /// of `self`. Note however that any previously added UTXOs are removed. Call
    /// [`replace_by_fee_with_rng`](crate::Wallet::replace_by_fee_with_rng) to finish
    /// building the PSBT.
    ///
    /// ## Note
    ///
    /// There should be no ancestry linking the elements of `txs`, since replacing an
    /// ancestor necessarily invalidates the descendant.
    pub fn replace_txs(self, txs: &[Arc<Transaction>]) -> PsbtParams<Rbf> {
        let mut params = self.into_replace_params();
        params.replace(txs);
        params
    }

    /// Transition this [`PsbtParams`] to the [`Rbf`] state.
    fn into_replace_params(self) -> PsbtParams<Rbf> {
        PsbtParams {
            set: self.set,
            utxos: self.utxos,
            inputs: self.inputs,
            assets: self.assets,
            recipients: self.recipients,
            change_descriptor: self.change_descriptor,
            feerate: self.feerate,
            longterm_feerate: self.longterm_feerate,
            drain_wallet: self.drain_wallet,
            coin_selection: self.coin_selection,
            canonical_params: self.canonical_params,
            utxo_filter: self.utxo_filter,
            maturity_height: self.maturity_height,
            manually_selected_only: self.manually_selected_only,
            version: self.version,
            locktime: self.locktime,
            fallback_sequence: self.fallback_sequence,
            ordering: self.ordering,
            only_witness_utxo: self.only_witness_utxo,
            add_global_xpubs: self.add_global_xpubs,
            replace: self.replace,
            marker: core::marker::PhantomData,
        }
    }
}

impl<C> PsbtParams<C> {
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

    /// Only include inputs that are selected manually using [`add_utxos`] or [`add_planned_input`].
    ///
    /// Since the wallet will skip coin selection for additional candidates, the manually selected
    /// inputs must be enough to fund the transaction or else an error will be thrown due to
    /// insufficient funds.
    ///
    /// [`add_utxos`]: PsbtParams::add_utxos
    /// [`add_planned_input`]: PsbtParams::add_planned_input
    pub fn manually_selected_only(&mut self) -> &mut Self {
        self.manually_selected_only = true;
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

    /// Add outgoing recipients to the transaction.
    ///
    /// - `recipients`: An iterator of `(S, Amount)` tuples where `S` can be a [`bitcoin::Address`],
    ///   a script pubkey, or anything that can be converted straight into a [`ScriptBuf`].
    pub fn add_recipients<I, S>(&mut self, recipients: I) -> &mut Self
    where
        I: IntoIterator<Item = (S, Amount)>,
        S: Into<ScriptBuf>,
    {
        self.recipients
            .extend(recipients.into_iter().map(|(s, amt)| (s.into(), amt)));
        self
    }

    /// Set the transaction `nLockTime`.
    ///
    /// This can be used as a fallback in case none of the inputs to the transaction require an
    /// absolute locktime. If no locktime is required and nothing is specified here, then the
    /// locktime is set to the last known chain tip.
    pub fn locktime(&mut self, locktime: absolute::LockTime) -> &mut Self {
        self.locktime = Some(locktime);
        self
    }

    /// Set the height to be used when evaluating the maturity of coinbase outputs during coin
    /// selection.
    pub fn maturity_height(&mut self, height: absolute::Height) -> &mut Self {
        self.maturity_height = Some(height.to_consensus_u32());
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

    /// Set the parameters for modifying the wallet's view of canonical transactions.
    ///
    /// The `params` can be used to resolve conflicts manually, or to assert that a particular
    /// transaction should be treated as canonical for the purpose of building the current PSBT.
    /// Refer to [`CanonicalizationParams`] for more.
    pub fn canonicalization_params(
        &mut self,
        params: bdk_chain::CanonicalizationParams,
    ) -> &mut Self {
        self.canonical_params = params;
        self
    }

    /// Set the definite descriptor used for generating the change output.
    pub fn change_descriptor(&mut self, desc: DefiniteDescriptor) -> &mut Self {
        self.change_descriptor = Some(desc);
        self
    }

    /// Filter [`FullTxOut`]s by the provided closure.
    ///
    /// This option can be used to mark specific outputs unspendable or apply custom UTXO
    /// filtering logic.
    ///
    /// Any txouts for which the `predicate` returns `false` will be excluded from coin selection,
    /// otherwise any coin in the wallet that is mature and spendable will be eligible for
    /// selection.
    pub fn filter_utxos<F>(&mut self, predicate: F) -> &mut Self
    where
        F: Fn(&FullTxOut<ConfirmationBlockTime>) -> bool + Send + Sync + 'static,
    {
        self.utxo_filter = UtxoFilter(Arc::new(predicate));
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
    /// See [`Input`] for more on how to create inputs manually. Be aware that creating inputs
    /// in this manner relies on certain assumptions, like the UTXO validity, the satisfaction
    /// weight, and so on. As such you should only use this method to add inputs you definitely
    /// trust the values for.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdk_tx::Input;
    /// # use bdk_wallet::psbt::PsbtParams;
    /// # use bitcoin::{psbt, OutPoint, Sequence, TxOut};
    /// # let outpoint = OutPoint::null();
    /// # let sequence = Sequence::ENABLE_LOCKTIME_NO_RBF;
    /// # let psbt_input = psbt::Input::default();
    /// # let satisfaction_weight = 0;
    /// # let tx_status = None;
    /// # let is_coinbase = false;
    /// let mut params = PsbtParams::default();
    /// let input = Input::from_psbt_input(
    ///     outpoint,
    ///     sequence,
    ///     psbt_input,
    ///     satisfaction_weight,
    ///     tx_status,
    ///     is_coinbase,
    /// )?;
    /// params.add_planned_input(input);
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    ///
    /// [`Plan`]: miniscript::plan::Plan
    /// [`psbt::Input`]: bitcoin::psbt::Input
    pub fn add_planned_input(&mut self, input: Input) -> &mut Self {
        if self.set.insert(input.prev_outpoint()) {
            self.inputs.push(input);
        }
        self
    }

    /// Only fill in the [`witness_utxo`] field of PSBT inputs which spends funds under segwit (v0).
    ///
    /// This allows opting out of including the [`non_witness_utxo`] for segwit spends. This reduces
    /// the size of the PSBT, however be aware that some signers might require the presence of the
    /// `non_witness_utxo`.
    ///
    /// [`witness_utxo`]: bitcoin::psbt::Input::witness_utxo
    /// [`non_witness_utxo`]: bitcoin::psbt::Input::non_witness_utxo
    pub fn only_witness_utxo(&mut self) -> &mut Self {
        self.only_witness_utxo = true;
        self
    }

    /// Drain wallet.
    ///
    /// This will force selection of the available input candidates. As such, the option is only
    /// applied to inputs that meet the spending criteria.
    pub fn drain_wallet(&mut self) -> &mut Self {
        self.drain_wallet = true;
        self
    }

    /// Set the transaction [`Version`].
    pub fn version(&mut self, version: Version) -> &mut Self {
        self.version = Some(version);
        self
    }

    /// Set the [`Sequence`] value to be used as a fallback if not specified by the input.
    pub fn fallback_sequence(&mut self, sequence: Sequence) -> &mut Self {
        self.fallback_sequence = Some(sequence);
        self
    }

    // TODO(@valuedmammal): Should we expose an option to set the `longterm_feerate`, and/or
    // set the coin-select `ChangePolicy`?

    /// Fill in the global [`Psbt::xpub`]s field with the extended keys of the wallet's
    /// descriptors.
    ///
    /// Some offline signers and/or multisig wallets may require this.
    ///
    /// [`Psbt::xpub`]: bitcoin::Psbt::xpub
    pub fn add_global_xpubs(&mut self) -> &mut Self {
        self.add_global_xpubs = true;
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
#[allow(clippy::type_complexity)]
pub(crate) struct UtxoFilter(
    pub Arc<dyn Fn(&FullTxOut<ConfirmationBlockTime>) -> bool + Send + Sync>,
);

impl Default for UtxoFilter {
    fn default() -> Self {
        Self(Arc::new(|_| true))
    }
}

impl fmt::Debug for UtxoFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UtxoFilter")
    }
}

impl PsbtParams<Rbf> {
    /// Replace spends of the provided `txs`. This will internally set the list of UTXOs
    /// to be spent.
    fn replace(&mut self, txs: &[Arc<Transaction>]) {
        self.utxos.clear();
        self.set.clear();
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
        self.utxos
            .extend(utxos.iter().copied().filter(|&op| self.set.insert(op)));
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

    // Test that `replace_txs` maintains the expected params.
    #[test]
    fn test_replace_params() {
        use crate::KeychainKind::Internal;
        let (wallet, txid0) = crate::test_utils::get_funded_wallet_wpkh();
        let outpoint_0 = OutPoint::new(txid0, 0);
        let change_desc = wallet
            .public_descriptor(Internal)
            .at_derivation_index(0)
            .unwrap();

        // Create psbt
        let mut params = PsbtParams::default();
        params.change_descriptor(change_desc);
        params.drain_wallet();
        let (psbt, _) = wallet.create_psbt(params).unwrap();
        let tx = psbt.unsigned_tx;
        let txid1 = tx.compute_txid();

        // Replace tx
        let mut params = PsbtParams::default().replace_txs(&[Arc::new(tx)]);
        params.add_recipients([(ScriptBuf::new_op_return([0xb1, 0x0c]), Amount::ZERO)]);
        params.feerate(FeeRate::from_sat_per_vb_unchecked(8));

        // Get utxos
        assert_eq!(params.utxos(), &[outpoint_0].into());

        assert_eq!(params.replace, [txid1].into());
        assert_eq!(params.feerate, FeeRate::from_sat_per_vb_unchecked(8));
        assert_eq!(
            params.recipients,
            [(ScriptBuf::new_op_return([0xb1, 0x0c]), Amount::ZERO)]
        );

        // Remove utxo
        params.remove_utxo(&outpoint_0);
        assert!(params.utxos().is_empty());
        assert!(params.utxos.is_empty());
    }

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
        let params = PsbtParams::new().replace_txs(&txs);
        assert_eq!(params.set, expect_spends);
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
        assert!(params.utxos.contains(&op));

        params = PsbtParams::default();

        // Try adding duplicates in the same set.
        params.add_utxos(&[op, op, op]);
        assert_eq!(
            params.utxos(),
            &[op].into(),
            "Failed to filter duplicate outpoints"
        );
        assert!(params.utxos.contains(&op));
    }
}

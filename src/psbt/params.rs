//! Parameters for PSBT building.

use alloc::sync::Arc;
use alloc::vec::Vec;

use bdk_chain::{BlockId, CanonicalizationParams, TxGraph};
use bdk_tx::DefiniteDescriptor;
use bitcoin::{
    absolute, transaction::Version, Amount, FeeRate, OutPoint, ScriptBuf, Sequence, Transaction,
    Txid,
};
use miniscript::plan::Assets;

use crate::collections::HashSet;

/// Parameters to create a PSBT.
#[derive(Debug)]
pub struct PsbtParams {
    // Inputs
    pub(crate) utxos: HashSet<OutPoint>,

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

    // PSBT
    pub(crate) version: Option<Version>,
    pub(crate) locktime: Option<absolute::LockTime>,
    pub(crate) fallback_sequence: Option<Sequence>,
}

impl Default for PsbtParams {
    fn default() -> Self {
        Self {
            utxos: Default::default(),
            assets: Default::default(),
            recipients: Default::default(),
            change_descriptor: Default::default(),
            feerate: bitcoin::FeeRate::BROADCAST_MIN,
            longterm_feerate: bitcoin::FeeRate::from_sat_per_vb_unchecked(10),
            drain_wallet: Default::default(),
            coin_selection: Default::default(),
            canonical_params: Default::default(),
            version: Default::default(),
            locktime: Default::default(),
            fallback_sequence: Default::default(),
        }
    }
}

impl PsbtParams {
    /// Add UTXOs by outpoint to fund the transaction.
    pub fn add_utxos(&mut self, outpoints: &[OutPoint]) -> &mut Self {
        self.utxos.extend(outpoints);
        self
    }

    /// Get the currently selected spends.
    pub fn utxos(&self) -> &HashSet<OutPoint> {
        &self.utxos
    }

    /// Remove a UTXO from the currently selected inputs.
    pub fn remove_utxo(&mut self, outpoint: OutPoint) -> &mut Self {
        self.utxos.remove(&outpoint);
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
    /// Construct from PSBT `params` and an iterator of `txs` to replace.
    pub(crate) fn new(txs: &[Arc<Transaction>], params: PsbtParams) -> Self {
        Self {
            inner: params,
            ..Default::default()
        }
        .replace(txs)
    }

    /// Replace spends of the provided `txs`. This will internally set the inner
    /// params UTXOs to be spent.
    pub fn replace(self, txs: &[Arc<Transaction>]) -> Self {
        let txs: Vec<Arc<Transaction>> = txs.to_vec();
        let mut txids: HashSet<Txid> = txs.iter().map(|tx| tx.compute_txid()).collect();
        let mut tx_graph = TxGraph::<BlockId>::default();
        let mut utxos: HashSet<OutPoint> = HashSet::new();

        for tx in txs {
            let _ = tx_graph.insert_tx(tx);
        }

        // Sanitize the RBF set by removing elements of `txs` which have ancestors
        // in the same set. This is to avoid spending outputs of txs that are bound
        // for replacement.
        for tx_node in tx_graph.full_txs() {
            let tx = &tx_node.tx;
            if tx.is_coinbase()
                || tx_graph
                    .walk_ancestors(Arc::clone(tx), |_, tx| Some(tx.compute_txid()))
                    .any(|ancestor_txid| txids.contains(&ancestor_txid))
            {
                txids.remove(&tx_node.txid);
            } else {
                utxos.extend(tx.input.iter().map(|txin| txin.previous_output));
            }
        }

        Self {
            inner: PsbtParams {
                utxos,
                ..self.inner
            },
            replace: txids,
        }
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
        self.inner.utxos()
    }

    /// Remove a UTXO from the currently selected inputs.
    pub fn remove_utxo(&mut self, outpoint: OutPoint) -> &mut Self {
        self.inner.remove_utxo(outpoint);
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
        // To replace: { [A, B], [C] } (where B spends from A)
        // We can't replace the inputs of B, since we're already replacing A
        // therefore the inputs should only include the spends of [A, C].

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
        assert_eq!(params.utxos(), &expect_spends);
        assert_eq!(params.replace, [txid_a, txid_c].into());
    }
}

//! Parameters for creating a PSBT.
//!
//! PSBT building is split into three stages:
//!
//! 1. **Candidate construction** — [`CandidateParams`] configures which coins may fund the
//!    transaction; [`Wallet::candidates_with`] resolves them into a [`CandidateSet`]. A replacement
//!    (RBF) is just a candidate set built from options whose [`replace`] list is non-empty (or via
//!    the [`Wallet::rbf_candidates`] shortcut).
//! 2. **Selection** — [`SelectParams`] describes the recipients, fee rate and coin-selection
//!    strategy; it is passed alongside a [`CandidateSet`] to [`Wallet::select`], which runs coin
//!    selection and returns a [`bdk_tx::TxTemplate`]. To sweep (drain), use no recipients with
//!    [`SelectionStrategy::DrainAll`].
//! 3. **Emission** — the caller shapes the [`bdk_tx::TxTemplate`] (version, locktime, ordering,
//!    anti-fee-sniping) using its own methods, then emits the final [`Psbt`](bitcoin::Psbt) via
//!    [`Wallet::finish`] with [`FinishParams`].
//!
//! [`replace`]: CandidateParams::replace
//! [`Wallet::candidates_with`]: crate::Wallet::candidates_with
//! [`Wallet::rbf_candidates`]: crate::Wallet::rbf_candidates
//! [`Wallet::select`]: crate::Wallet::select
//! [`Wallet::finish`]: crate::Wallet::finish

use alloc::vec::Vec;

use bdk_chain::CanonicalizationParams;
use bdk_tx::{ChangeScript, Input, InputCandidates, RbfParams};
use bitcoin::{absolute, Amount, FeeRate, OutPoint, ScriptBuf, Txid};
use miniscript::plan::Assets;

use crate::collections::{BTreeSet, HashSet};
use crate::types::LocalOutput;
use crate::wallet::error::CandidatesError;

/// Parameters for building the set of spendable input candidates (PSBT-building stage 1).
///
/// Configures how candidates are derived **from the wallet** — manually selected ("must spend")
/// UTXOs, the spend [`Assets`], canonicalization, and Replace-By-Fee. Pass it to
/// [`Wallet::candidates_with`] to resolve a [`CandidateSet`].
///
/// All fields are public; construct with [`new`](Self::new) (or [`Default`]) and set what you
/// need. Manually-selected [`must_spend`](Self::must_spend) outpoints are de-duplicated when the
/// [`CandidateSet`] is resolved.
///
/// To spend a UTXO that did not originate from this wallet (a pre-built foreign [`Input`]), don't
/// configure it here — push it onto the resolved [`CandidateSet`] with
/// [`push_must_select`](CandidateSet::push_must_select) /
/// [`push_can_select`](CandidateSet::push_can_select).
///
/// To build a replacement transaction (RBF), list the txids to replace in
/// [`replace`](Self::replace); the resulting [`CandidateSet`] carries the replacement context
/// forward to stage 2, so any output shape (pay or sweep) can replace.
///
/// [`Wallet::candidates_with`]: crate::Wallet::candidates_with
#[derive(Debug, Default)]
pub struct CandidateParams {
    /// Manually-selected UTXO outpoints that must be spent.
    ///
    /// Each outpoint must correspond to an output of a transaction tracked by the wallet and be
    /// currently unspent, otherwise resolving the [`CandidateSet`] yields [`UnknownUtxo`]. To spend
    /// a UTXO that did not originate from this wallet, push a foreign [`Input`] onto the resolved
    /// [`CandidateSet`] instead (see [`CandidateSet::push_must_select`]).
    ///
    /// [`UnknownUtxo`]: crate::wallet::error::CandidatesError::UnknownUtxo
    pub must_spend: BTreeSet<OutPoint>,
    /// Spend [`Assets`] used to create spending plans for the wallet's own outputs.
    ///
    /// An empty value (the default) means no assets are provided, in which case all keys are
    /// assumed equally likely to sign.
    pub assets: Assets,
    /// Parameters for modifying the wallet's view of canonical transactions.
    ///
    /// Refer to [`CanonicalizationParams`] for more.
    pub canonical_params: CanonicalizationParams,
    /// Height used when evaluating the maturity of coinbase outputs during coin selection.
    ///
    /// Defaults to the chain tip height when `None`.
    pub maturity_height: Option<absolute::Height>,
    /// Only include inputs selected manually via [`must_spend`](Self::must_spend) (plus any foreign
    /// inputs pushed onto the resolved [`CandidateSet`]); skip coin selection for additional
    /// candidates.
    ///
    /// The manually-selected inputs must then be enough to fund the transaction.
    pub manually_selected_only: bool,
    /// Txids to replace (Replace-By-Fee).
    ///
    /// The must-spend inputs of the resulting [`CandidateSet`] are derived from the inputs of the
    /// replaced transactions (resolved against the wallet's transaction graph). There should be no
    /// ancestry linking these txids — replacing an ancestor invalidates the descendant — and such
    /// ancestry is sanitized away during resolution.
    pub replace: Vec<Txid>,
}

impl CandidateParams {
    /// Create new, empty [`CandidateParams`].
    pub fn new() -> Self {
        Self::default()
    }
}

/// A resolved set of spendable input candidates (output of PSBT-building stage 1).
///
/// Produced by [`Wallet::candidates_with`] from [`CandidateParams`]: every owned UTXO has been
/// planned against the wallet's descriptors and spendability filters applied. It owns its inputs
/// (no wallet borrow), so it can be held as a snapshot and used to build one or more PSBTs via
/// [`Wallet::select`].
///
/// Add foreign (non-wallet) inputs with [`push_must_select`](Self::push_must_select) /
/// [`push_can_select`](Self::push_can_select), and apply your own post-resolution filters with
/// [`filter`](Self::filter) / [`regroup`](Self::regroup).
///
/// If the [`CandidateParams`] had a non-empty [`replace`](CandidateParams::replace) list, the set
/// carries the [`bdk_tx::RbfParams`] (replaced-tx fee statistics) forward so stage 2 applies the
/// correct fee floor, and exposes the wallet-owned outputs being stripped by the replacement via
/// [`replaced_unspent`](Self::replaced_unspent) (handy for batching the replaced txs' payments).
///
/// [`Wallet::candidates_with`]: crate::Wallet::candidates_with
/// [`Wallet::select`]: crate::Wallet::select
#[derive(Debug, Clone)]
pub struct CandidateSet {
    pub(crate) candidates: InputCandidates,
    pub(crate) rbf: Option<RbfParams>,
    /// Txids being replaced/evicted (direct conflicts + descendants). A pushed input may not spend
    /// an output of any of these.
    pub(crate) replaced: HashSet<Txid>,
    /// Wallet-owned UTXOs stripped from the canonical view by the replacement.
    pub(crate) replaced_unspent: Vec<LocalOutput>,
}

impl CandidateSet {
    /// Iterate over all resolved input candidates (both must-select and optional).
    pub fn inputs(&self) -> impl Iterator<Item = &Input> + '_ {
        self.candidates.inputs()
    }

    /// Whether the set contains no candidates at all.
    pub fn is_empty(&self) -> bool {
        self.candidates.inputs().next().is_none()
    }

    /// Whether this set is a Replace-By-Fee set (built from a non-empty
    /// [`CandidateParams::replace`] list).
    pub fn is_rbf(&self) -> bool {
        self.rbf.is_some()
    }

    /// Wallet-owned UTXOs that the replacement strips out of the canonical view — the outputs of
    /// the replaced (and descendant) txs that were unspent in the wallet's view before the replace.
    ///
    /// These are the still-live payments of the txs being replaced; a caller batching several txs
    /// into one replacement can use them to decide which payments to re-create. Empty for a
    /// non-Replace-By-Fee set.
    pub fn replaced_unspent(&self) -> &[LocalOutput] {
        &self.replaced_unspent
    }

    /// Add a foreign [`Input`] to the must-select group (always spent).
    ///
    /// Use this for a UTXO that did not originate from the wallet, supplied with a pre-built
    /// [`Plan`]/[`psbt::Input`] — its validity (UTXO existence, satisfaction weight, ...) relies on
    /// the caller-supplied values, so only push inputs you trust.
    ///
    /// # Errors
    ///
    /// Returns [`ConflictingInput`] if the input spends an output of a transaction being replaced
    /// (RBF) — that output won't exist after the replacement.
    ///
    /// [`ConflictingInput`]: CandidatesError::ConflictingInput
    /// [`Plan`]: miniscript::plan::Plan
    /// [`psbt::Input`]: bitcoin::psbt::Input
    pub fn push_must_select(mut self, input: Input) -> Result<Self, CandidatesError> {
        self.ensure_not_replaced(&input)?;
        self.candidates = self.candidates.push_must_select(input);
        Ok(self)
    }

    /// Add a foreign [`Input`] as an optional (can-select) candidate.
    ///
    /// Like [`push_must_select`](Self::push_must_select), but the input is offered to coin
    /// selection rather than always spent.
    ///
    /// # Errors
    ///
    /// Returns [`ConflictingInput`](CandidatesError::ConflictingInput) if the input spends an
    /// output of a transaction being replaced (RBF).
    pub fn push_can_select(mut self, input: Input) -> Result<Self, CandidatesError> {
        self.ensure_not_replaced(&input)?;
        self.candidates = self.candidates.push_can_select(input);
        Ok(self)
    }

    /// Reject an input that spends an output of a replaced (evicted) transaction.
    fn ensure_not_replaced(&self, input: &Input) -> Result<(), CandidatesError> {
        let op = input.prev_outpoint();
        if self.replaced.contains(&op.txid) {
            return Err(CandidatesError::ConflictingInput(op));
        }
        Ok(())
    }

    /// Keep only the candidates for which `policy` returns `true`.
    ///
    /// Forwards to [`bdk_tx::InputCandidates::filter`]. The closure receives each
    /// [`bdk_tx::Input`], which exposes enough to filter by value, script, or confirmation: e.g.
    /// [`prev_txout`](Input::prev_txout) (amount/script), [`status`](Input::status) and
    /// [`confirmations`](Input::confirmations) (confirmed-only: `|i| i.status().is_some()`),
    /// [`is_coinbase`](Input::is_coinbase), and [`is_immature`](Input::is_immature).
    pub fn filter<P>(mut self, policy: P) -> Self
    where
        P: FnMut(&Input) -> bool,
    {
        self.candidates = self.candidates.filter(policy);
        self
    }

    /// Regroup the candidates by the group key returned by `policy`.
    ///
    /// Forwards to [`bdk_tx::InputCandidates::regroup`].
    pub fn regroup<P, G>(mut self, policy: P) -> Self
    where
        P: FnMut(&Input) -> G,
        G: Ord + Clone,
    {
        self.candidates = self.candidates.regroup(policy);
        self
    }

    /// Consume into the underlying `bdk_tx` parts: the [`InputCandidates`] and, if this is a
    /// Replace-By-Fee set (see [`is_rbf`](Self::is_rbf)), the [`RbfParams`] carrying the
    /// replaced-tx fee floor.
    ///
    /// Pass both on to `bdk_tx` (e.g. via [`SelectorParams::replace`]) to build a PSBT directly
    /// while still enforcing the RBF minimum fee. The `RbfParams` is wallet-derived and cannot be
    /// reconstructed without the wallet, so it is returned here rather than dropped.
    ///
    /// [`SelectorParams::replace`]: bdk_tx::SelectorParams::replace
    pub fn into_parts(self) -> (InputCandidates, Option<RbfParams>) {
        (self.candidates, self.rbf)
    }
}

/// Parameters to create a PSBT that pays a set of recipients (PSBT-building stage 2).
///
/// Built with [`SelectParams::new`], passed alongside a [`CandidateSet`] to
/// [`Wallet::select`], which runs coin selection and returns a [`bdk_tx::TxTemplate`]. The caller
/// then shapes the template (version, locktime, anti-fee-sniping, input/output ordering) using
/// the template's own methods before emitting the PSBT via [`Wallet::finish`].
///
/// [`Wallet::select`]: crate::Wallet::select
/// [`Wallet::finish`]: crate::Wallet::finish
#[derive(Debug)]
pub struct SelectParams {
    /// List of recipient script/amount pairs.
    pub recipients: Vec<(ScriptBuf, Amount)>,
    /// Optional script or descriptor designated for change.
    pub change_script: Option<ChangeScript>,
    /// Coin selection strategy to use.
    ///
    /// Defaults to [`SelectionStrategy::SingleRandomDraw`]. Use [`SelectionStrategy::DrainAll`]
    /// (with no recipients) to sweep the whole candidate set.
    pub coin_selection: SelectionStrategy,
    /// Target fee rate.
    pub fee_rate: FeeRate,
}

impl Default for SelectParams {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectParams {
    /// Create `SelectParams` with no recipients, default coin selection, and the
    /// `FeeRate::BROADCAST_MIN` fee rate.
    pub fn new() -> Self {
        Self {
            recipients: Vec::new(),
            change_script: None,
            coin_selection: SelectionStrategy::default(),
            fee_rate: FeeRate::BROADCAST_MIN,
        }
    }
}

/// Parameters for emitting the final [`Psbt`] from a [`bdk_tx::TxTemplate`] (PSBT-building stage 3).
///
/// Carries only PSBT-emission options. Transaction-shape decisions (version, locktime, sequence,
/// anti-fee-sniping, input/output ordering) live on the [`bdk_tx::TxTemplate`] returned by
/// [`Wallet::select`] and are applied with the template's own methods before being passed to
/// [`Wallet::finish`].
///
/// [`Psbt`]: bitcoin::Psbt
/// [`Wallet::select`]: crate::Wallet::select
/// [`Wallet::finish`]: crate::Wallet::finish
#[derive(Debug, Clone, Default)]
pub struct FinishParams {
    /// Only set the [`witness_utxo`](bitcoin::psbt::Input::witness_utxo) in segwit-v0 PSBT inputs.
    pub only_witness_utxo: bool,
    /// Whether to try filling in the PSBT global xpubs from the wallet's descriptors.
    pub add_global_xpubs: bool,
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
    /// [`LowestFee`] metric for more.
    ///
    /// [`LowestFee`]: bdk_tx::bdk_coin_select::metrics::LowestFee
    LowestFee {
        /// Hypothetical average long-term feerate of the change spending transaction.
        longterm_feerate: FeeRate,
        /// How many times to run BnB before giving up.
        max_rounds: usize,
    },
    /// Select **all** available candidates (drain), ignoring any target amount.
    ///
    /// The remainder (everything minus fees) goes to the change output. With no recipients this
    /// sweeps the whole candidate set to change (auto-derived if no `change_script` is set, or an
    /// explicit destination); with recipients it pays them and sends the rest to change
    /// ("drain while paying").
    DrainAll,
}

/// Merge the available signing keys and hash preimages from `src` into `dst`.
///
/// Only these additive (set-union) secrets are merged. The absolute/relative **timelocks are
/// deliberately left untouched** — they are single-valued ceilings with no unambiguous merge (and
/// [`absolute::LockTime`]/[`relative::LockTime`] are only partially ordered, so there is no
/// well-defined "stricter" across a height- and a time-based lock). Callers that care set the
/// timelocks explicitly after merging.
///
/// [`relative::LockTime`]: bitcoin::relative::LockTime
pub(crate) fn merge_assets_secrets(dst: &mut Assets, src: &Assets) {
    dst.keys.extend(src.keys.clone());
    dst.sha256_preimages.extend(src.sha256_preimages.clone());
    dst.hash256_preimages.extend(src.hash256_preimages.clone());
    dst.ripemd160_preimages
        .extend(src.ripemd160_preimages.clone());
    dst.hash160_preimages.extend(src.hash160_preimages.clone());
}

//! Parameters for PSBT building.

use alloc::vec::Vec;

use bdk_tx::DefiniteDescriptor;
use bitcoin::{absolute, transaction::Version, Amount, FeeRate, OutPoint, ScriptBuf, Sequence};

/// Parameters to create a PSBT.
#[derive(Debug, Clone)]
pub struct Params {
    // Inputs
    pub(crate) utxos: Vec<OutPoint>,
    // TODO: miniscript plan Assets?
    // pub(crate) assets: Assets,

    // Outputs
    pub(crate) recipients: Vec<(ScriptBuf, Amount)>,
    pub(crate) change_descriptor: Option<DefiniteDescriptor>,

    // Coin Selection
    pub(crate) feerate: FeeRate,
    pub(crate) longterm_feerate: FeeRate,
    pub(crate) drain_wallet: bool,
    pub(crate) coin_selection: SelectionStrategy,

    // PSBT
    pub(crate) version: Option<Version>,
    pub(crate) locktime: Option<absolute::LockTime>,
    pub(crate) fallback_sequence: Option<Sequence>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            utxos: Default::default(),
            recipients: Default::default(),
            change_descriptor: Default::default(),
            feerate: bitcoin::FeeRate::BROADCAST_MIN,
            longterm_feerate: bitcoin::FeeRate::from_sat_per_vb_unchecked(10),
            drain_wallet: Default::default(),
            coin_selection: Default::default(),
            version: Default::default(),
            locktime: Default::default(),
            fallback_sequence: Default::default(),
        }
    }
}

// TODO: more setters for Params
impl Params {
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
}

/// Coin select strategy.
#[derive(Debug, Clone, Copy, Default)]
pub enum SelectionStrategy {
    /// Single random draw.
    #[default]
    SingleRandomDraw,
    /// Lowest fee, a variation of Branch 'n Bound that allows for change
    /// while minimizing transaction fees. Refer to
    /// [`LowestFee`](bdk_coin_select::metrics::LowestFee) metric for more.
    LowestFee,
}

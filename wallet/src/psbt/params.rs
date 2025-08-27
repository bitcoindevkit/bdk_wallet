//! Parameters for PSBT building.

use alloc::vec::Vec;

use bdk_tx::DefiniteDescriptor;
use bitcoin::{absolute, transaction::Version, Amount, FeeRate, OutPoint, ScriptBuf, Sequence};
use miniscript::plan::Assets;

/// Parameters to create a PSBT.
#[derive(Debug)]
pub struct Params {
    // Inputs
    pub(crate) utxos: Vec<OutPoint>,

    // Outputs
    pub(crate) recipients: Vec<(ScriptBuf, Amount)>,
    pub(crate) change_descriptor: Option<DefiniteDescriptor>,

    // Coin Selection
    pub(crate) assets: Option<Assets>,
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
            assets: Default::default(),
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
    /// Add the spend [`Assets`].
    ///
    /// Assets are required to create a spending plan for an output controlled by the wallet's
    /// descriptors. If none are provided here, then we assume all of the keys are equally likely
    /// to sign.
    ///
    /// This may be called multiple times to add additional assets, however only the last
    /// absolute or relative timelock is retained. See also `AssetsExt`.
    pub fn add_assets<I, S>(&mut self, assets: Assets) -> &mut Self {
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

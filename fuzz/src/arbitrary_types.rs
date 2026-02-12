//! Arbitrary types for structure-aware fuzzing
//!
//! This module provides wrapper types that implement the Arbitrary trait
//! for efficient structure-aware fuzzing of BDK wallet components.

use arbitrary::{Arbitrary, Result, Unstructured};
use bdk_wallet::bitcoin::{
    hashes::Hash, psbt::PsbtSighashType, Amount, BlockHash, OutPoint,
    ScriptBuf, Txid,
};
use bdk_wallet::{signer::TapLeavesOptions, SignOptions, TxOrdering};

/// A fuzzed transaction ID
#[derive(Arbitrary, Debug, Clone)]
pub struct FuzzedTxid([u8; 32]);

impl FuzzedTxid {
    pub fn into_txid(self) -> Txid {
        Txid::from_byte_array(self.0)
    }
}

/// A fuzzed block hash
#[derive(Arbitrary, Debug, Clone)]
pub struct FuzzedBlockHash([u8; 32]);

impl FuzzedBlockHash {
    pub fn into_block_hash(self) -> BlockHash {
        BlockHash::from_byte_array(self.0)
    }
}

/// A fuzzed outpoint (transaction output reference)
#[derive(Arbitrary, Debug, Clone)]
pub struct FuzzedOutPoint {
    txid: FuzzedTxid,
    vout: u32,
}

impl FuzzedOutPoint {
    pub fn into_outpoint(self) -> OutPoint {
        OutPoint::new(self.txid.into_txid(), self.vout)
    }
}

/// A fuzzed amount in satoshis with reasonable constraints
#[derive(Debug, Clone)]
pub struct FuzzedAmount(u64);

impl Arbitrary<'_> for FuzzedAmount {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Generate amounts between 0 and 21 million BTC in satoshis
        // Use smaller amounts more frequently for better test coverage
        let max_sats = 21_000_000 * 100_000_000u64;
        let amount = if u.ratio(9, 10)? {
            // 90% of the time use smaller amounts (up to 1000 BTC)
            u.int_in_range(0..=100_000_000_000)?
        } else {
            // 10% of the time use any amount up to max supply
            u.int_in_range(0..=max_sats)?
        };
        Ok(FuzzedAmount(amount))
    }
}

impl FuzzedAmount {
    pub fn into_amount(self) -> Amount {
        Amount::from_sat(self.0)
    }

    pub fn as_sats(&self) -> u64 {
        self.0
    }
}

/// A fuzzed script with size constraints
#[derive(Debug, Clone)]
pub struct FuzzedScript(Vec<u8>);

impl Arbitrary<'_> for FuzzedScript {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Generate scripts with reasonable size limits
        // Most scripts are small, occasionally generate larger ones
        let max_len = if u.ratio(9, 10)? {
            100  // 90% of the time, small scripts
        } else {
            520  // 10% of the time, up to standard max script size
        };

        let len = u.int_in_range(0..=max_len)?;
        let mut bytes = vec![0u8; len];
        u.fill_buffer(&mut bytes)?;
        Ok(FuzzedScript(bytes))
    }
}

impl FuzzedScript {
    pub fn into_script(self) -> ScriptBuf {
        ScriptBuf::from_bytes(self.0)
    }
}

/// Wallet actions that can be performed during fuzzing
#[derive(Arbitrary, Debug, Clone)]
pub enum FuzzedWalletAction {
    /// Apply an update to the wallet
    ApplyUpdate,
    /// Create and sign a transaction
    CreateTx,
    /// Persist wallet state and reload it
    PersistAndLoad,
}

/// Fuzzed signing options for wallet operations
#[derive(Debug, Clone)]
pub struct FuzzedSignOptions {
    pub trust_witness_utxo: bool,
    pub assume_height: Option<u32>,
    pub allow_all_sighashes: bool,
    pub try_finalize: bool,
    pub tap_leaves_options: FuzzedTapLeavesOptions,
    pub sign_with_tap_internal_key: bool,
    pub allow_grinding: bool,
}

impl Arbitrary<'_> for FuzzedSignOptions {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(FuzzedSignOptions {
            trust_witness_utxo: u.arbitrary()?,
            assume_height: if u.arbitrary()? {
                Some(u.int_in_range(0..=2_000_000)?)  // Reasonable block height range
            } else {
                None
            },
            allow_all_sighashes: u.arbitrary()?,
            try_finalize: u.arbitrary()?,
            tap_leaves_options: u.arbitrary()?,
            sign_with_tap_internal_key: u.arbitrary()?,
            allow_grinding: u.arbitrary()?,
        })
    }
}

impl FuzzedSignOptions {
    pub fn into_sign_options(self) -> SignOptions {
        SignOptions {
            trust_witness_utxo: self.trust_witness_utxo,
            assume_height: self.assume_height,
            allow_all_sighashes: self.allow_all_sighashes,
            try_finalize: self.try_finalize,
            tap_leaves_options: self.tap_leaves_options.into_tap_leaves_options(),
            sign_with_tap_internal_key: self.sign_with_tap_internal_key,
            allow_grinding: self.allow_grinding,
        }
    }
}

/// Taproot leaves signing options
#[derive(Arbitrary, Debug, Clone)]
pub enum FuzzedTapLeavesOptions {
    /// Sign all taproot leaves
    All,
    /// Don't sign any taproot leaves
    None,
    // TODO: Add Include/Exclude variants with specific leaf hashes when needed
}

impl FuzzedTapLeavesOptions {
    pub fn into_tap_leaves_options(self) -> TapLeavesOptions {
        match self {
            FuzzedTapLeavesOptions::All => TapLeavesOptions::All,
            FuzzedTapLeavesOptions::None => TapLeavesOptions::None,
        }
    }
}

/// Options for building transactions
#[derive(Debug, Clone)]
pub struct FuzzedTxBuilderOptions {
    pub fee_rate: Option<u64>,        // Satoshis per vbyte
    pub fee_absolute: Option<u64>,    // Absolute fee in satoshis
    pub manually_selected_only: bool,
    pub sighash: Option<PsbtSighashType>,
    pub ordering: FuzzedTxOrdering,
    pub locktime: Option<u32>,
    pub version: Option<i32>,
    pub do_not_spend_change: bool,
    pub only_spend_change: bool,
    pub only_witness_utxo: bool,
    pub include_output_redeem_witness_script: bool,
    pub add_global_xpubs: bool,
    pub drain_wallet: bool,
    pub allow_dust: bool,
}

impl Arbitrary<'_> for FuzzedTxBuilderOptions {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(FuzzedTxBuilderOptions {
            fee_rate: if u.ratio(1, 3)? {
                // Use reasonable fee rates (1-1000 sat/vb)
                Some(u.int_in_range(1..=1000)?)
            } else {
                None
            },
            fee_absolute: if u.ratio(1, 10)? {
                // Absolute fees up to 0.01 BTC
                Some(u.int_in_range(0..=1_000_000)?)
            } else {
                None
            },
            manually_selected_only: u.arbitrary()?,
            sighash: if u.ratio(1, 10)? {
                // Occasionally set custom sighash
                Some(PsbtSighashType::from_u32(u.int_in_range(0..=0x83)?))
            } else {
                None
            },
            ordering: u.arbitrary()?,
            locktime: if u.ratio(1, 5)? {
                Some(u.arbitrary()?)
            } else {
                None
            },
            version: if u.ratio(1, 10)? {
                Some(u.int_in_range(1..=2)?)
            } else {
                None
            },
            do_not_spend_change: u.ratio(1, 20)?,  // Rare option
            only_spend_change: u.ratio(1, 20)?,     // Rare option
            only_witness_utxo: u.arbitrary()?,
            include_output_redeem_witness_script: u.arbitrary()?,
            add_global_xpubs: u.arbitrary()?,
            drain_wallet: u.ratio(1, 10)?,
            allow_dust: u.ratio(1, 5)?,
        })
    }
}

/// Transaction ordering options
#[derive(Arbitrary, Debug, Clone)]
pub enum FuzzedTxOrdering {
    Shuffle,
    Untouched,
    // BIP69 could be added here if needed
}

impl FuzzedTxOrdering {
    pub fn into_tx_ordering(self) -> TxOrdering {
        match self {
            FuzzedTxOrdering::Shuffle => TxOrdering::Shuffle,
            FuzzedTxOrdering::Untouched => TxOrdering::Untouched,
        }
    }
}
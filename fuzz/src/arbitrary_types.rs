//! Arbitrary types for structure-aware fuzzing
//!
//! This module provides wrapper types that implement the Arbitrary trait
//! for efficient structure-aware fuzzing of BDK wallet components.

use arbitrary::{Arbitrary, Result, Unstructured};
use bdk_wallet::bitcoin::{
    hashes::Hash, Amount, BlockHash, OutPoint, ScriptBuf, Txid,
};

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
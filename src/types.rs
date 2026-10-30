// Bitcoin Dev Kit
// Written in 2020 by Alekos Filini <alekos.filini@gmail.com>
//
// Copyright (c) 2020-2021 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use alloc::boxed::Box;
use chain::{ChainPosition, ConfirmationBlockTime};
use core::convert::AsRef;
use core::fmt;

use crate::collections::BTreeMap;

use bitcoin::transaction::{OutPoint, Sequence, TxOut};
use bitcoin::{Weight, psbt};

use serde::{Deserialize, Serialize};

/// Types of keychains
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum KeychainKind {
    /// External keychain, used for deriving recipient addresses.
    External = 0,
    /// Internal keychain, used for deriving change addresses.
    Internal = 1,
}

/// Stored as the text `"external"` / `"internal"` so a database is readable and stays valid if
/// the enum's discriminants ever change.
#[cfg(feature = "rusqlite")]
impl bdk_chain::rusqlite::ToSql for KeychainKind {
    fn to_sql(&self) -> bdk_chain::rusqlite::Result<bdk_chain::rusqlite::types::ToSqlOutput<'_>> {
        let s = match self {
            KeychainKind::External => "external",
            KeychainKind::Internal => "internal",
        };
        Ok(bdk_chain::rusqlite::types::ToSqlOutput::from(s))
    }
}

#[cfg(feature = "rusqlite")]
impl bdk_chain::rusqlite::types::FromSql for KeychainKind {
    fn column_result(
        value: bdk_chain::rusqlite::types::ValueRef<'_>,
    ) -> bdk_chain::rusqlite::types::FromSqlResult<Self> {
        match value.as_str()? {
            "external" => Ok(KeychainKind::External),
            "internal" => Ok(KeychainKind::Internal),
            other => Err(bdk_chain::rusqlite::types::FromSqlError::Other(
                alloc::boxed::Box::new(UnknownKeychain(alloc::string::String::from(other))),
            )),
        }
    }
}

/// A keychain column held a value this wallet does not recognise.
#[cfg(feature = "rusqlite")]
#[derive(Debug)]
pub struct UnknownKeychain(pub alloc::string::String);

#[cfg(feature = "rusqlite")]
impl core::fmt::Display for UnknownKeychain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown keychain: {}", self.0)
    }
}

#[cfg(feature = "rusqlite")]
impl core::error::Error for UnknownKeychain {}

impl KeychainKind {
    /// Return [`KeychainKind`] as a byte
    pub fn as_byte(&self) -> u8 {
        match self {
            KeychainKind::External => b'e',
            KeychainKind::Internal => b'i',
        }
    }
}

impl fmt::Display for KeychainKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeychainKind::External => write!(f, "External"),
            KeychainKind::Internal => write!(f, "Internal"),
        }
    }
}

impl AsRef<[u8]> for KeychainKind {
    fn as_ref(&self) -> &[u8] {
        match self {
            KeychainKind::External => b"e",
            KeychainKind::Internal => b"i",
        }
    }
}

/// An unspent output owned by a [`Wallet`].
///
/// [`Wallet`]: crate::Wallet
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalOutput<K> {
    /// Reference to a transaction output
    pub outpoint: OutPoint,
    /// Transaction output
    pub txout: TxOut,
    /// Type of keychain
    pub keychain: K,
    /// Whether this UTXO is spent or not
    pub is_spent: bool,
    /// The derivation index for the script pubkey in the wallet
    pub derivation_index: u32,
    /// The position of the output in the blockchain.
    pub chain_position: ChainPosition<ConfirmationBlockTime>,
}

/// A [`Utxo`] with its `satisfaction_weight`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightedUtxo {
    /// The weight of the witness data and `scriptSig` expressed in [weight units]. This is used to
    /// properly maintain the feerate when adding this input to a transaction during coin
    /// selection.
    ///
    /// [weight units]: https://en.bitcoin.it/wiki/Weight_units
    pub satisfaction_weight: Weight,
    /// The UTXO
    pub utxo: Utxo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An unspent transaction output (UTXO).
pub enum Utxo {
    /// A UTXO owned by the local wallet.
    Local(LocalOutput<KeychainKind>),
    /// A UTXO owned by another wallet.
    Foreign {
        /// The location of the output.
        outpoint: OutPoint,
        /// The nSequence value to set for this input.
        sequence: Sequence,
        /// The information about the input we require to add it to a PSBT.
        // Box it to stop the type being too big.
        psbt_input: Box<psbt::Input>,
    },
}

impl Utxo {
    /// Get the location of the UTXO
    pub fn outpoint(&self) -> OutPoint {
        match &self {
            Utxo::Local(local) => local.outpoint,
            Utxo::Foreign { outpoint, .. } => *outpoint,
        }
    }

    /// Get the `TxOut` of the UTXO
    pub fn txout(&self) -> &TxOut {
        match &self {
            Utxo::Local(local) => &local.txout,
            Utxo::Foreign {
                outpoint,
                psbt_input,
                ..
            } => psbt_input.witness_utxo.as_ref().unwrap_or_else(|| {
                psbt_input
                    .non_witness_utxo
                    .as_ref()
                    .and_then(|tx| tx.output.get(outpoint.vout as usize))
                    .expect("Foreign UTXOs should have one of witness_utxo, non_witness_utxo set")
            }),
        }
    }

    /// Get the sequence number if an explicit sequence number has to be set for this input.
    pub fn sequence(&self) -> Option<Sequence> {
        match self {
            Utxo::Local(_) => None,
            Utxo::Foreign { sequence, .. } => Some(*sequence),
        }
    }
}

/// The finalization status for a single PSBT input.
#[derive(Debug, PartialEq)]
pub enum FinalizeInputOutcome {
    /// The input was already finalized before this call.
    AlreadyFinalized,
    /// The input was successfully finalized during this call.
    Finalized,
    /// The wallet could not derive a descriptor for the input.
    MissingDescriptor,
    /// The wallet found the descriptor but could not construct the input satisfaction.
    CouldNotSatisfy(miniscript::Error),
}

impl FinalizeInputOutcome {
    /// Whether the input is finalized after this call.
    pub fn is_finalized(&self) -> bool {
        matches!(self, Self::AlreadyFinalized | Self::Finalized)
    }
}

/// The outcome of a PSBT finalization attempt.
#[derive(Debug, PartialEq)]
pub struct FinalizePsbtOutcome {
    outcomes: BTreeMap<usize, FinalizeInputOutcome>,
}

impl FinalizePsbtOutcome {
    pub(crate) fn new(outcomes: BTreeMap<usize, FinalizeInputOutcome>) -> Self {
        Self { outcomes }
    }

    /// Whether all inputs are finalized after this call.
    pub fn is_finalized(&self) -> bool {
        self.outcomes
            .values()
            .all(FinalizeInputOutcome::is_finalized)
    }

    /// Borrow the per-input finalization outcomes.
    pub fn outcomes(&self) -> &BTreeMap<usize, FinalizeInputOutcome> {
        &self.outcomes
    }

    /// Consume the collection and return the per-input finalization outcomes.
    pub fn into_outcomes(self) -> BTreeMap<usize, FinalizeInputOutcome> {
        self.outcomes
    }
}

/// Index out of bounds error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOutOfBoundsError {
    /// The index that is out of range.
    pub index: usize,
    /// The length of the container.
    pub len: usize,
}

impl IndexOutOfBoundsError {
    /// Create a new `IndexOutOfBoundsError`.
    pub fn new(index: usize, len: usize) -> Self {
        Self { index, len }
    }
}

impl fmt::Display for IndexOutOfBoundsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Index out of bounds: index {} is greater than or equal to length {}",
            self.index, self.len
        )
    }
}

impl core::error::Error for IndexOutOfBoundsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{
        Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness, absolute,
        transaction,
    };

    fn build_tx(txout: TxOut) -> Transaction {
        Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::default(),
                sequence: Sequence::MAX,
                witness: Witness::default(),
            }],
            output: vec![txout],
        }
    }

    #[test]
    fn txout_foreign_returns_witness_utxo() {
        let txout = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: ScriptBuf::default(),
        };
        let utxo = Utxo::Foreign {
            outpoint: OutPoint::null(),
            sequence: Sequence::MAX,
            psbt_input: Box::new(psbt::Input {
                witness_utxo: Some(txout.clone()),
                ..Default::default()
            }),
        };
        assert_eq!(utxo.txout(), &txout);
    }

    #[test]
    fn txout_foreign_returns_non_witness_utxo() {
        let txout = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: ScriptBuf::default(),
        };
        let prev_tx = build_tx(txout.clone());
        let utxo = Utxo::Foreign {
            outpoint: OutPoint {
                txid: prev_tx.compute_txid(),
                vout: 0,
            },
            sequence: Sequence::MAX,
            psbt_input: Box::new(psbt::Input {
                non_witness_utxo: Some(prev_tx),
                ..Default::default()
            }),
        };
        assert_eq!(utxo.txout(), &txout);
    }

    #[test]
    #[should_panic(
        expected = "Foreign UTXOs should have one of witness_utxo, non_witness_utxo set"
    )]
    fn txout_foreign_panics_with_empty_psbt_input() {
        let utxo = Utxo::Foreign {
            outpoint: OutPoint::null(),
            sequence: Sequence::MAX,
            psbt_input: Box::new(psbt::Input::default()),
        };
        utxo.txout();
    }
}

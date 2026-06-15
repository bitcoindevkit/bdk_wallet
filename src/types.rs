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
#[cfg(feature = "elias-fano")]
use alloc::string::String;
use alloc::vec::Vec;
use chain::{ChainPosition, ConfirmationBlockTime};
use core::convert::AsRef;
use core::fmt;

use bitcoin::transaction::{OutPoint, Sequence, TxOut};
use bitcoin::{psbt, Weight};

use serde::{Deserialize, Serialize};

/// Types of keychains
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum KeychainKind {
    /// External keychain, used for deriving recipient addresses.
    External = 0,
    /// Internal keychain, used for deriving change addresses.
    Internal = 1,
}

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
pub struct LocalOutput {
    /// Reference to a transaction output
    pub outpoint: OutPoint,
    /// Transaction output
    pub txout: TxOut,
    /// Type of keychain
    pub keychain: KeychainKind,
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
    Local(LocalOutput),
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

/// Derivation index metadata for a single keychain descriptor.
///
/// Captures the sorted list of used derivation indexes as flat integer data
/// suitable for encoding and export formats.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpkMetadata {
    /// The keychain this metadata belongs to.
    keychain: KeychainKind,
    /// Sorted derivation indexes that have been used (have on-chain `TxOut`s).
    used_indexes: Vec<u32>,
}

impl SpkMetadata {
    /// Build script pubkey metadata for a keychain.
    ///
    /// The provided indexes are zero-based BDK derivation indexes. They are
    /// normalized by sorting and removing duplicates.
    pub fn new(keychain: KeychainKind, used_indexes: impl Into<Vec<u32>>) -> Self {
        let mut used_indexes = used_indexes.into();
        used_indexes.sort_unstable();
        used_indexes.dedup();

        Self {
            keychain,
            used_indexes,
        }
    }

    /// Return the keychain this metadata belongs to.
    pub fn keychain(&self) -> KeychainKind {
        self.keychain
    }

    /// Return the sorted zero-based derivation indexes with known wallet activity.
    pub fn used_indexes(&self) -> &[u32] {
        &self.used_indexes
    }

    /// Return whether this metadata contains no used indexes.
    pub fn is_empty(&self) -> bool {
        self.used_indexes.is_empty()
    }

    /// Consume this metadata and return the used indexes.
    pub fn into_used_indexes(self) -> Vec<u32> {
        self.used_indexes
    }

    /// Build [`SpkMetadata`] from a [`KeychainTxOutIndex`] for the given keychain.
    ///
    /// The collected indexes are normalized through [`SpkMetadata::new`].
    ///
    /// [`KeychainTxOutIndex`]: chain::indexer::keychain_txout::KeychainTxOutIndex
    pub fn from_index(
        index: &chain::indexer::keychain_txout::KeychainTxOutIndex<KeychainKind>,
        keychain: KeychainKind,
    ) -> Self {
        let used_indexes: Vec<u32> = index
            .keychain_outpoints(keychain)
            .map(|(idx, _)| idx)
            .collect();

        Self::new(keychain, used_indexes)
    }
}

#[cfg(feature = "elias-fano")]
impl SpkMetadata {
    /// Encode `used_indexes` as an Elias-Fano representation.
    ///
    /// Returns `None` if there are no used indexes.
    pub fn encode_elias_fano(&self) -> Option<sux::prelude::EliasFano> {
        use sux::prelude::EliasFanoBuilder;

        let n = self.used_indexes.len();
        if n == 0 {
            return None;
        }
        let upper_bound = *self.used_indexes.last().unwrap() as usize + 1;

        let mut efb = EliasFanoBuilder::new(n, upper_bound);
        for &idx in &self.used_indexes {
            efb.push(idx as usize);
        }
        Some(efb.build())
    }

    /// Encode `used_indexes` as an Elias-Fano representation serialized to a
    /// base64 string.
    ///
    /// Returns `None` if there are no used indexes.
    pub fn encode_base64(&self) -> Option<String> {
        use bitcoin::base64::prelude::{Engine as _, BASE64_STANDARD};

        let ef = self.encode_elias_fano()?;
        let json = serde_json::to_vec(&ef).expect("EliasFano serialization must not fail");
        Some(BASE64_STANDARD.encode(&json))
    }

    /// Decode a base64-encoded Elias-Fano representation back into [`SpkMetadata`].
    ///
    /// Returns `None` if the input is empty or decoding fails.
    pub fn decode_base64(b64: &str, keychain: KeychainKind) -> Option<Self> {
        use bitcoin::base64::prelude::{Engine as _, BASE64_STANDARD};

        let json_bytes = BASE64_STANDARD.decode(b64).ok()?;
        let ef: sux::prelude::EliasFano = serde_json::from_slice(&json_bytes).ok()?;
        let used_indexes: Vec<u32> = ef.into_iter().map(|v| v as u32).collect();

        Some(Self::new(keychain, used_indexes))
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
    use std::vec;

    use super::*;
    use bitcoin::{
        absolute, transaction, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut,
        Witness,
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
    fn test_spk_metadata_construction() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![0, 1, 3]);
        assert_eq!(meta.keychain(), KeychainKind::External);
        assert_eq!(meta.used_indexes(), &[0, 1, 3]);
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_elias_fano_round_trip() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![0, 2, 5, 7]);

        // Encode to EliasFano and verify values
        let ef = meta.encode_elias_fano().unwrap();
        let decoded: Vec<usize> = ef.into_iter().collect();
        assert_eq!(decoded, vec![0, 2, 5, 7]);

        // Round-trip through base64
        let b64 = meta.encode_base64().unwrap();
        let decoded_meta = SpkMetadata::decode_base64(&b64, KeychainKind::External).unwrap();
        assert_eq!(decoded_meta, meta);
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_elias_fano_empty() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![]);
        assert!(meta.encode_elias_fano().is_none());
        assert!(meta.encode_base64().is_none());
        assert!(SpkMetadata::decode_base64("", KeychainKind::External).is_none());
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

    #[test]
    fn test_spk_metadata_new_normalizes_used_indexes() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![50, 0, 20, 20]);

        assert_eq!(meta.keychain(), KeychainKind::External);
        assert_eq!(meta.used_indexes(), &[0, 20, 50]);
    }

    #[test]
    fn test_spk_metadata_preserves_zero_based_indexes() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![0, 1, 3]);
        assert_eq!(meta.used_indexes(), &[0, 1, 3]);
    }

    #[test]
    fn test_spk_metadata_empty() {
        let meta = SpkMetadata::new(KeychainKind::Internal, Vec::new());

        assert_eq!(meta.keychain(), KeychainKind::Internal);
        assert!(meta.is_empty());
    }

    #[test]
    fn test_spk_metadata_into_used_indexes() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![3, 1, 1]);

        assert_eq!(meta.into_used_indexes(), vec![1, 3]);
    }
}

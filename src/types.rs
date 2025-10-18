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

use bitcoin::transaction::{OutPoint, Sequence, TxOut};
use bitcoin::{psbt, Weight};

use serde::{Deserialize, Serialize};

#[cfg(feature = "rusqlite")]
use chain::rusqlite::{
    self,
    types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef},
};

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

#[cfg(feature = "rusqlite")]
impl FromSql for KeychainKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(match value.as_str()? {
            "0" => KeychainKind::External,
            "1" => KeychainKind::Internal,
            _ => panic!("KeychainKind cannot be anything other than External(0) and Internal(1)"),
        })
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for KeychainKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match *self {
            KeychainKind::External => "0".into(),
            KeychainKind::Internal => "1".into(),
        })
    }
}

#[cfg(feature = "rusqlite")]
use crate::CanBePersisted;

#[cfg(feature = "rusqlite")]
impl CanBePersisted for KeychainKind {
    type Persistable = KeychainKind;
    fn to_persistable(self) -> KeychainKind {
        self
    }
    fn from_persistable(persisted: KeychainKind) -> KeychainKind {
        persisted
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
            } => {
                if let Some(prev_tx) = &psbt_input.non_witness_utxo {
                    return &prev_tx.output[outpoint.vout as usize];
                }

                if let Some(txout) = &psbt_input.witness_utxo {
                    return txout;
                }

                unreachable!("Foreign UTXOs will always have one of these set")
            }
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

#[cfg(feature = "std")]
impl std::error::Error for IndexOutOfBoundsError {}

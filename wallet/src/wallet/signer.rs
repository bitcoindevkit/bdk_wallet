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

use alloc::string::String;
use core::fmt;

use bitcoin::{psbt, sighash};

use crate::wallet::error::MiniscriptPsbtError;

/// Signing error
#[derive(Debug)]
pub enum SignerError {
    /// The private key is missing for the required public key
    MissingKey,
    /// The private key in use has the right fingerprint but derives differently than expected
    InvalidKey,
    /// The user canceled the operation
    UserCanceled,
    /// Input index is out of range
    InputIndexOutOfRange,
    /// The `non_witness_utxo` field of the transaction is required to sign this input
    MissingNonWitnessUtxo,
    /// The `non_witness_utxo` specified is invalid
    InvalidNonWitnessUtxo,
    /// The `witness_utxo` field of the transaction is required to sign this input
    MissingWitnessUtxo,
    /// The `witness_script` field of the transaction is required to sign this input
    MissingWitnessScript,
    /// The fingerprint and derivation path are missing from the psbt input
    MissingHdKeypath,
    /// The psbt contains a non-`SIGHASH_ALL` sighash in one of its input and the user hasn't
    /// explicitly allowed them
    ///
    /// To enable signing transactions with non-standard sighashes set
    /// [`SignOptions::allow_all_sighashes`] to `true`.
    NonStandardSighash,
    /// Invalid SIGHASH for the signing context in use
    InvalidSighash,
    /// Error while computing the hash to sign a Taproot input.
    SighashTaproot(sighash::TaprootError),
    /// PSBT sign error.
    Psbt(psbt::SignError),
    /// Miniscript PSBT error
    MiniscriptPsbt(MiniscriptPsbtError),
    /// To be used only by external libraries implementing [`InputSigner`] or
    /// [`TransactionSigner`], so that they can return their own custom errors, without having to
    /// modify [`SignerError`] in BDK.
    External(String),
}

impl fmt::Display for SignerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingKey => write!(f, "Missing private key"),
            Self::InvalidKey => write!(f, "The private key in use has the right fingerprint but derives differently than expected"),
            Self::UserCanceled => write!(f, "The user canceled the operation"),
            Self::InputIndexOutOfRange => write!(f, "Input index out of range"),
            Self::MissingNonWitnessUtxo => write!(f, "Missing non-witness UTXO"),
            Self::InvalidNonWitnessUtxo => write!(f, "Invalid non-witness UTXO"),
            Self::MissingWitnessUtxo => write!(f, "Missing witness UTXO"),
            Self::MissingWitnessScript => write!(f, "Missing witness script"),
            Self::MissingHdKeypath => write!(f, "Missing fingerprint and derivation path"),
            Self::NonStandardSighash => write!(f, "The psbt contains a non standard sighash"),
            Self::InvalidSighash => write!(f, "Invalid SIGHASH for the signing context in use"),
            Self::SighashTaproot(err) => write!(f, "Error while computing the hash to sign a Taproot input: {err}"),
            Self::Psbt(err) => write!(f, "Error computing the sighash: {err}"),
            Self::MiniscriptPsbt(err) => write!(f, "Miniscript PSBT error: {err}"),
            Self::External(err) => write!(f, "{err}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SignerError {}

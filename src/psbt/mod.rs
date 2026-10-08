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

//! Additional functions on the `rust-bitcoin` `Psbt` structure.

use alloc::vec::Vec;
use bitcoin::psbt;
use bitcoin::{Amount, FeeRate, OutPoint, Psbt, TxOut};

#[cfg(all(bdk_wallet_unstable, feature = "bdk-tx"))]
mod params;
#[cfg(all(bdk_wallet_unstable, feature = "bdk-tx"))]
pub use params::*;

pub(crate) fn validated_non_witness_prevout(
    input: &psbt::Input,
    outpoint: OutPoint,
) -> Option<&TxOut> {
    let prev_tx = input.non_witness_utxo.as_ref()?;
    if prev_tx.compute_txid() != outpoint.txid {
        return None;
    }
    prev_tx.output.get(outpoint.vout as usize)
}

// TODO: Upstream these PSBT utilities to rust-bitcoin.

/// Trait to add functions to extract utxos and calculate fees.
pub trait PsbtUtils {
    /// Get the `TxOut` for the specified input index, if it doesn't exist in the PSBT `None` is
    /// returned.
    fn get_utxo_for(&self, input_index: usize) -> Option<TxOut>;

    /// The total transaction fee amount, sum of input amounts minus sum of output amounts, in sats.
    /// Returns `None` if a TxOut is missing for an input, if summing amounts overflows, or if the
    /// outputs exceed the inputs.
    fn fee_amount(&self) -> Option<Amount>;

    /// The transaction's fee rate. This value will only be accurate if calculated AFTER the
    /// `Psbt` is finalized and all witness/signature data is added to the
    /// transaction.
    /// Returns `None` if a TxOut is missing for an input, if summing amounts overflows, if the
    /// outputs exceed the inputs, or if the transaction cannot be extracted.
    fn fee_rate(&self) -> Option<FeeRate>;
}

impl PsbtUtils for Psbt {
    fn get_utxo_for(&self, input_index: usize) -> Option<TxOut> {
        let tx = &self.unsigned_tx;
        let input = self.inputs.get(input_index)?;
        let outpoint = tx.input.get(input_index)?.previous_output;

        match (&input.witness_utxo, &input.non_witness_utxo) {
            (Some(witness_utxo), Some(_)) => {
                let non_witness_utxo = validated_non_witness_prevout(input, outpoint)?;
                (witness_utxo == non_witness_utxo).then(|| witness_utxo.clone())
            }
            (_, Some(_)) => validated_non_witness_prevout(input, outpoint).cloned(),
            (Some(_), _) => input.witness_utxo.clone(),
            _ => None,
        }
    }

    fn fee_amount(&self) -> Option<Amount> {
        let tx = &self.unsigned_tx;
        let utxos: Option<Vec<TxOut>> = (0..tx.input.len()).map(|i| self.get_utxo_for(i)).collect();

        utxos.and_then(|inputs| {
            let input_amount = inputs
                .iter()
                .map(|i| i.value)
                .try_fold(Amount::ZERO, Amount::checked_add)?;
            let output_amount = self
                .unsigned_tx
                .output
                .iter()
                .map(|o| o.value)
                .try_fold(Amount::ZERO, Amount::checked_add)?;
            input_amount.checked_sub(output_amount)
        })
    }

    fn fee_rate(&self) -> Option<FeeRate> {
        let fee_amount = self.fee_amount();
        let weight = self.clone().extract_tx().ok()?.weight();
        fee_amount.map(|fee| fee / weight)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::psbt::Input;
    use bitcoin::{
        Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness, absolute,
        transaction,
    };

    /// Builds a simple transaction with one output of the given value
    fn build_tx(value: Amount) -> Transaction {
        Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::default(),
                sequence: Sequence::MAX,
                witness: Witness::default(),
            }],
            output: vec![TxOut {
                value,
                script_pubkey: ScriptBuf::default(),
            }],
        }
    }

    /// Builds a PSBT spending from the given previous transaction at the given vout
    fn build_psbt(prev_tx: &Transaction, vout: u32) -> Psbt {
        let unsigned_tx = Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: prev_tx.compute_txid(),
                    vout,
                },
                script_sig: ScriptBuf::default(),
                sequence: Sequence::MAX,
                witness: Witness::default(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(90_000),
                script_pubkey: ScriptBuf::default(),
            }],
        };
        Psbt::from_unsigned_tx(unsigned_tx).unwrap()
    }

    #[test]
    fn get_utxo_for_returns_none_on_txid_mismatch() {
        let real_tx = build_tx(Amount::from_sat(100_000));

        // A different transaction with an inflated value — simulates attacker input
        let fake_tx = build_tx(Amount::from_sat(999_999_999));

        // PSBT spends from real_tx but attacker supplies fake_tx as non_witness_utxo
        let mut psbt = build_psbt(&real_tx, 0);
        psbt.inputs[0] = Input {
            non_witness_utxo: Some(fake_tx), // txid won't match
            ..Default::default()
        };

        // Must return None — fake tx rejected
        assert_eq!(psbt.get_utxo_for(0), None);
    }

    #[test]
    fn get_utxo_for_returns_none_on_vout_out_of_bounds() {
        let prev_tx = build_tx(Amount::from_sat(100_000));
        // prev_tx only has 1 output (vout 0), but we claim to spend vout 3
        let mut psbt = build_psbt(&prev_tx, 3);
        psbt.inputs[0] = Input {
            non_witness_utxo: Some(prev_tx), // txid matches, but vout 3 doesn't exist
            ..Default::default()
        };

        // Must return None — vout out of bounds, no panic
        assert_eq!(psbt.get_utxo_for(0), None);
    }

    #[test]
    fn fee_amount_returns_input_minus_output() {
        let prev_tx = build_tx(Amount::from_sat(100_000));
        let mut psbt = build_psbt(&prev_tx, 0);
        psbt.inputs[0] = Input {
            non_witness_utxo: Some(prev_tx),
            ..Default::default()
        };

        assert_eq!(psbt.fee_amount(), Some(Amount::from_sat(10_000)));
    }

    #[test]
    fn fee_amount_returns_none_when_outputs_exceed_inputs() {
        let prev_tx = build_tx(Amount::from_sat(50_000));
        // build_psbt creates a 90_000 sat output
        let mut psbt = build_psbt(&prev_tx, 0);
        psbt.inputs[0] = Input {
            non_witness_utxo: Some(prev_tx),
            ..Default::default()
        };

        assert_eq!(psbt.fee_amount(), None);
        assert_eq!(psbt.fee_rate(), None);
    }

    #[test]
    fn fee_amount_returns_none_when_input_amounts_overflow() {
        let unsigned_tx = Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![
                TxIn {
                    previous_output: OutPoint::null(),
                    script_sig: ScriptBuf::default(),
                    sequence: Sequence::MAX,
                    witness: Witness::default(),
                },
                TxIn {
                    previous_output: OutPoint::null(),
                    script_sig: ScriptBuf::default(),
                    sequence: Sequence::MAX,
                    witness: Witness::default(),
                },
            ],
            output: vec![TxOut {
                value: Amount::from_sat(1_000),
                script_pubkey: ScriptBuf::default(),
            }],
        };
        let mut psbt = Psbt::from_unsigned_tx(unsigned_tx).unwrap();
        for input in &mut psbt.inputs {
            input.witness_utxo = Some(TxOut {
                value: Amount::from_sat(u64::MAX),
                script_pubkey: ScriptBuf::default(),
            });
        }

        assert_eq!(psbt.fee_amount(), None);
        assert_eq!(psbt.fee_rate(), None);
    }

    #[test]
    fn fee_amount_returns_none_when_output_amounts_overflow() {
        let prev_tx = build_tx(Amount::from_sat(1_000));
        let unsigned_tx = Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: prev_tx.compute_txid(),
                    vout: 0,
                },
                script_sig: ScriptBuf::default(),
                sequence: Sequence::MAX,
                witness: Witness::default(),
            }],
            output: vec![
                TxOut {
                    value: Amount::from_sat(u64::MAX),
                    script_pubkey: ScriptBuf::default(),
                },
                TxOut {
                    value: Amount::from_sat(u64::MAX),
                    script_pubkey: ScriptBuf::default(),
                },
            ],
        };
        let mut psbt = Psbt::from_unsigned_tx(unsigned_tx).unwrap();
        psbt.inputs[0] = Input {
            non_witness_utxo: Some(prev_tx),
            ..Default::default()
        };

        assert_eq!(psbt.fee_amount(), None);
        assert_eq!(psbt.fee_rate(), None);
    }
}

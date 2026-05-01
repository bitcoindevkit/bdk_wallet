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
use bitcoin::Amount;
use bitcoin::FeeRate;
use bitcoin::Psbt;
use bitcoin::TxOut;

// TODO upstream the functions here to `rust-bitcoin`?

/// Trait to add functions to extract utxos and calculate fees.
pub trait PsbtUtils {
    /// Get the `TxOut` for the specified input index, if it doesn't exist in the PSBT `None` is
    /// returned.
    fn get_utxo_for(&self, input_index: usize) -> Option<TxOut>;

    /// The total transaction fee amount, sum of input amounts minus sum of output amounts, in sats.
    /// If the PSBT is missing a TxOut for an input returns None.
    fn fee_amount(&self) -> Option<Amount>;

    /// The transaction's fee rate. This value will only be accurate if calculated AFTER the
    /// `Psbt` is finalized and all witness/signature data is added to the
    /// transaction.
    /// If the PSBT is missing a TxOut for an input returns None.
    fn fee_rate(&self) -> Option<FeeRate>;
}

impl PsbtUtils for Psbt {
    fn get_utxo_for(&self, input_index: usize) -> Option<TxOut> {
        let tx = &self.unsigned_tx;
        let input = self.inputs.get(input_index)?;

        match (&input.witness_utxo, &input.non_witness_utxo) {
            (Some(_), _) => input.witness_utxo.clone(),
            (_, Some(_)) => input.non_witness_utxo.as_ref().and_then(|prev_tx| {
                let outpoint = tx.input[input_index].previous_output;
                if prev_tx.compute_txid() != outpoint.txid {
                    return None;
                }
                prev_tx.output.get(outpoint.vout as usize).cloned()
            }),
            _ => None,
        }
    }

    fn fee_amount(&self) -> Option<Amount> {
        let tx = &self.unsigned_tx;
        let utxos: Option<Vec<TxOut>> = (0..tx.input.len()).map(|i| self.get_utxo_for(i)).collect();

        utxos.map(|inputs| {
            let input_amount: Amount = inputs.iter().map(|i| i.value).sum();
            let output_amount: Amount = self.unsigned_tx.output.iter().map(|o| o.value).sum();
            input_amount
                .checked_sub(output_amount)
                .expect("input amount must be greater than output amount")
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
        absolute, transaction, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut,
        Witness,
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
}

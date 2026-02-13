//! Optimized arbitrary types for high-performance fuzzing
//!
//! This module provides performance-optimized types for structure-aware fuzzing

use arbitrary::{Arbitrary, Result, Unstructured};
use bdk_wallet::bitcoin::{
    absolute::LockTime, hashes::Hash, transaction::Version,
    Amount, BlockHash, FeeRate, ScriptBuf, Sequence, Transaction, TxIn, TxOut,
};
use bdk_wallet::{
    chain::{BlockId, ConfirmationBlockTime},
    rusqlite::Connection,
    KeychainKind, PersistedWallet, SignOptions, Update,
};
use std::collections::BTreeMap;
use std::sync::Arc;

// Import and re-export the base types from the original module
use crate::arbitrary_types::FuzzedOutPoint;
pub use crate::arbitrary_types::{EXTERNAL_DESCRIPTOR, INTERNAL_DESCRIPTOR, NETWORK};

/// Optimized transaction with fewer inputs/outputs
#[derive(Debug, Clone)]
pub struct OptimizedTransaction {
    pub version: i32,
    pub lock_time: u32,
    pub inputs: Vec<OptimizedTxInput>,
    pub outputs: Vec<OptimizedTxOutput>,
}

#[derive(Debug, Clone)]
pub struct OptimizedTxInput {
    pub previous_output: FuzzedOutPoint,
    pub sequence: u32,
}

#[derive(Debug, Clone)]
pub struct OptimizedTxOutput {
    pub value: u64,  // Direct sats, no wrapper
    pub use_wallet_script: bool,  // True = use wallet address, False = small script
}

impl Arbitrary<'_> for OptimizedTransaction {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Much smaller transactions: 1-3 inputs, 1-2 outputs
        let num_inputs = u.int_in_range(1..=3)?;
        let num_outputs = u.int_in_range(1..=2)?;

        let mut inputs = Vec::with_capacity(num_inputs);
        for _ in 0..num_inputs {
            inputs.push(OptimizedTxInput {
                previous_output: u.arbitrary()?,
                sequence: if u.ratio(1, 20)? {  // 5% special sequences
                    *u.choose(&[0xfffffffd, 0xfffffffe, 0xffffffff])?
                } else {
                    0xffffffff  // Default
                },
            });
        }

        let mut outputs = Vec::with_capacity(num_outputs);
        for _ in 0..num_outputs {
            outputs.push(OptimizedTxOutput {
                value: u.int_in_range(1000..=100_000)?,  // Smaller amounts
                use_wallet_script: u.ratio(1, 3)?,  // 33% wallet scripts
            });
        }

        Ok(OptimizedTransaction {
            version: if u.ratio(1, 10)? { 2 } else { 1 },
            lock_time: if u.ratio(1, 10)? { u.arbitrary()? } else { 0 },
            inputs,
            outputs,
        })
    }
}

impl OptimizedTransaction {
    pub fn into_transaction(self, wallet: &mut PersistedWallet<Connection>) -> Transaction {
        let inputs: Vec<TxIn> = self.inputs.into_iter().map(|input| {
            TxIn {
                previous_output: input.previous_output.into_outpoint(),
                sequence: Sequence(input.sequence),
                ..Default::default()
            }
        }).collect();

        let outputs: Vec<TxOut> = self.outputs.into_iter().map(|output| {
            let script = if output.use_wallet_script {
                wallet.next_unused_address(KeychainKind::External).script_pubkey()
            } else {
                // Simple P2WPKH-like script (22 bytes)
                let mut script_bytes = vec![0x00, 0x14];
                script_bytes.extend_from_slice(&[0; 20]);
                ScriptBuf::from_bytes(script_bytes)
            };
            TxOut {
                value: Amount::from_sat(output.value),
                script_pubkey: script,
            }
        }).collect();

        Transaction {
            version: Version(self.version),
            lock_time: LockTime::from_consensus(self.lock_time),
            input: inputs,
            output: outputs,
        }
    }
}

/// Optimized transaction update with smaller sizes
#[derive(Debug)]
pub struct OptimizedTxUpdate {
    pub txs: Vec<OptimizedTransaction>,
    pub has_anchors: bool,  // Just flag instead of full data
    pub has_seen_ats: bool,
}

impl Arbitrary<'_> for OptimizedTxUpdate {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Much smaller: 0-2 transactions
        let num_txs = u.int_in_range(0..=2)?;

        Ok(OptimizedTxUpdate {
            txs: (0..num_txs).map(|_| u.arbitrary()).collect::<Result<_>>()?,
            has_anchors: u.ratio(1, 4)?,  // 25% have anchors
            has_seen_ats: u.ratio(1, 4)?,  // 25% have seen times
        })
    }
}

impl OptimizedTxUpdate {
    pub fn into_tx_update(self, wallet: &mut PersistedWallet<Connection>) -> bdk_wallet::chain::TxUpdate<ConfirmationBlockTime> {
        let txs: Vec<Arc<Transaction>> = self.txs
            .into_iter()
            .map(|tx| Arc::new(tx.into_transaction(wallet)))
            .collect();

        let mut update = bdk_wallet::chain::TxUpdate::<ConfirmationBlockTime>::default();
        update.txs = txs;

        // Add minimal anchors if needed
        if self.has_anchors && !update.txs.is_empty() {
            let txid = update.txs[0].compute_txid();
            let anchor = ConfirmationBlockTime {
                block_id: BlockId {
                    height: 100,
                    hash: BlockHash::from_byte_array([0; 32]),
                },
                confirmation_time: 1_600_000_000,
            };
            update.anchors.insert((anchor, txid));
        }

        // Add minimal seen times if needed
        if self.has_seen_ats && !update.txs.is_empty() {
            let txid = update.txs[0].compute_txid();
            update.seen_ats.insert((txid, 1_600_000_000));
        }

        update
    }
}

/// Optimized wallet update
#[derive(Debug)]
pub struct OptimizedUpdate {
    pub has_indices: bool,  // Just flag instead of full data
    pub tx_update: OptimizedTxUpdate,
}

impl Arbitrary<'_> for OptimizedUpdate {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(OptimizedUpdate {
            has_indices: u.ratio(1, 5)?,  // 20% have indices
            tx_update: u.arbitrary()?,
        })
    }
}

impl OptimizedUpdate {
    pub fn into_update(self, wallet: &mut PersistedWallet<Connection>) -> Update {
        let mut last_active_indices = BTreeMap::new();

        if self.has_indices {
            // Add minimal indices
            last_active_indices.insert(KeychainKind::External, 1);
        }

        Update {
            last_active_indices,
            tx_update: self.tx_update.into_tx_update(wallet),
            chain: None,  // Skip chain updates for performance
        }
    }
}

/// Simplified transaction builder
#[derive(Debug, Clone)]
pub struct OptimizedTxBuilder {
    pub has_recipients: bool,
    pub fee_type: FeeType,
}

#[derive(Debug, Clone)]
pub enum FeeType {
    Default,
    Rate(u64),
    Absolute(u64),
}

impl Arbitrary<'_> for OptimizedTxBuilder {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(OptimizedTxBuilder {
            has_recipients: u.ratio(3, 4)?,  // 75% have recipients
            fee_type: match u.int_in_range(0..=2)? {
                0 => FeeType::Default,
                1 => FeeType::Rate(u.int_in_range(1..=50)?),  // Lower rates
                _ => FeeType::Absolute(u.int_in_range(100..=10000)?),  // Lower fees
            },
        })
    }
}

impl OptimizedTxBuilder {
    pub fn build_with_wallet(self, wallet: &mut PersistedWallet<Connection>) -> std::result::Result<bdk_wallet::bitcoin::psbt::Psbt, Box<dyn std::error::Error>> {
        // Get script before creating builder to avoid borrow conflict
        let recipient_script = if self.has_recipients {
            Some(wallet.next_unused_address(KeychainKind::External).script_pubkey())
        } else {
            None
        };

        let mut builder = wallet.build_tx();

        // Add simple recipient if needed
        if let Some(script) = recipient_script {
            builder.set_recipients(vec![(script, Amount::from_sat(1000))]);
        }

        // Set fee
        match self.fee_type {
            FeeType::Rate(rate) => {
                if let Some(fee_rate) = FeeRate::from_sat_per_vb(rate) {
                    builder.fee_rate(fee_rate);
                }
            }
            FeeType::Absolute(fee) => {
                builder.fee_absolute(Amount::from_sat(fee));
            }
            FeeType::Default => {}
        }

        builder.finish().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

/// Optimized wallet operation
#[derive(Debug)]
pub enum OptimizedWalletOperation {
    ApplyUpdate(OptimizedUpdate),
    CreateTransaction {
        builder: OptimizedTxBuilder,
        should_sign: bool,  // Simple flag instead of full options
    },
    // Skip PersistAndLoad - it's expensive and auto-persisted anyway
}

impl Arbitrary<'_> for OptimizedWalletOperation {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        match u.int_in_range(0..=1)? {
            0 => Ok(OptimizedWalletOperation::ApplyUpdate(u.arbitrary()?)),
            _ => Ok(OptimizedWalletOperation::CreateTransaction {
                builder: u.arbitrary()?,
                should_sign: u.ratio(1, 2)?,  // 50% sign
            }),
        }
    }
}

/// Optimized fuzzing input with limited operations
#[derive(Debug)]
pub struct OptimizedFuzzInput {
    pub operations: Vec<OptimizedWalletOperation>,
}

impl Arbitrary<'_> for OptimizedFuzzInput {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        // Limit to 1-5 operations per run for better performance
        let num_ops = u.int_in_range(1..=5)?;
        let mut operations = Vec::with_capacity(num_ops);

        for _ in 0..num_ops {
            operations.push(u.arbitrary()?);
        }

        Ok(OptimizedFuzzInput { operations })
    }
}

impl OptimizedFuzzInput {
    pub fn execute(
        self,
        wallet: &mut PersistedWallet<Connection>,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        for operation in self.operations {
            match operation {
                OptimizedWalletOperation::ApplyUpdate(update) => {
                    let update = update.into_update(wallet);
                    wallet.apply_update(update)?;
                }
                OptimizedWalletOperation::CreateTransaction { builder, should_sign } => {
                    let mut psbt = match builder.build_with_wallet(wallet) {
                        Ok(psbt) => psbt,
                        Err(_) => continue,
                    };

                    if should_sign {
                        // Use default sign options for performance
                        let _ = wallet.sign(&mut psbt, SignOptions::default());
                    }
                }
            }
        }
        Ok(())
    }
}
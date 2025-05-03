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
use chain::{ChainPosition, ConfirmationBlockTime, DescriptorExt, DescriptorId};
use std::hash::{Hash, Hasher};
use std::prelude::rust_2021::Vec;
use bitcoin::transaction::{OutPoint, Sequence, TxOut};
use bitcoin::{psbt, Network, Weight};
use miniscript::{Descriptor, DescriptorPublicKey};
use miniscript::descriptor::KeyMap;
use serde::{Deserialize, Serialize};
use crate::descriptor::{IntoWalletDescriptor};
use crate::DescriptorToExtract;
use crate::wallet::make_descriptor_to_extract;
use crate::wallet::utils::SecpCtx;

/// Types of keychains
// #[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
// pub enum KeychainKind {
//     /// External keychain, used for deriving recipient addresses.
//     External = 0,
//     /// Internal keychain, used for deriving change addresses.
//     Internal = 1,
// }

// impl KeychainKind {
//     /// Return [`KeychainKind`] as a byte
//     pub fn as_byte(&self) -> u8 {
//         match self {
//             KeychainKind::External => b'e',
//             KeychainKind::Internal => b'i',
//         }
//     }
// }
//
// impl AsRef<[u8]> for KeychainKind {
//     fn as_ref(&self) -> &[u8] {
//         match self {
//             KeychainKind::External => b"e",
//             KeychainKind::Internal => b"i",
//         }
//     }
// }

#[derive(Clone, Debug, Copy, Eq, Ord, PartialEq, Hash, Serialize, Deserialize, PartialOrd)]
pub enum KeychainKind {
    Default,
    Change,
    Other(DescriptorId),
}

// pub type KeychainIdentifier = (KeychainKind, Option<DescriptorId>);

pub type WalletKeychain = (KeychainKind, (Descriptor<DescriptorPublicKey>, KeyMap));

/// A `WalletKeychain` is mostly a descriptor with metadata associated with it. It states whether the
/// keychain is the default keychain for the wallet, and provides an identifier for it which can be
/// used for retrieval.
// #[derive(Clone, Debug, Eq, PartialEq)]
// pub struct WalletKeychain {
//     pub keychain_kind: KeychainKind,
//     pub public_descriptor: Descriptor<DescriptorPublicKey>,
//     pub keymap: KeyMap,
// }

#[derive(Debug, Clone)]
pub struct KeyRing {
    keychains: Vec<WalletKeychain>,
    network: Network,
}

impl KeyRing {
    pub fn new<D: IntoWalletDescriptor + Send + 'static>(
        default_descriptor: D,
        network: Network,
    ) -> Self {
        let secp = SecpCtx::new();
        let descriptor_to_extract: DescriptorToExtract = make_descriptor_to_extract(default_descriptor);
        let public_descriptor: (Descriptor<DescriptorPublicKey>, KeyMap) = descriptor_to_extract(&secp, network).unwrap();
        let descriptor_id: DescriptorId = public_descriptor.0.descriptor_id();
        // Using the type alias
        let wallet_keychain = ((KeychainKind::Default), public_descriptor);

        // Using the struct
        // let wallet_keychain = WalletKeychain {
        //     keychain_kind: KeychainKind::Default(descriptor_id),
        //     public_descriptor,
        //     keymap: KeyMap::default()
        // };

        KeyRing {
            keychains: vec![wallet_keychain],
            network,
        }
    }

    // TODO: This needs to never fail because there is always a default keychain.
    pub fn get_default_keychain(&self) -> WalletKeychain {
        self.keychains.iter().find(|keychain| matches!(keychain.0, KeychainKind::Default)).unwrap().clone()
    }

    pub fn get_change_keychain(&self) -> Option<WalletKeychain> {
        self.keychains.iter().find(|keychain| matches!(keychain.0, KeychainKind::Change)).cloned()
    }

    pub fn add_other_descriptor<D: IntoWalletDescriptor + Send + 'static>(
        &mut self,
        other_descriptor: D
    ) -> &mut KeyRing {
        let secp = SecpCtx::new();
        let descriptor_to_extract: DescriptorToExtract = make_descriptor_to_extract(other_descriptor);
        let public_descriptor = descriptor_to_extract(&secp, self.network).unwrap();
        let descriptor_id = public_descriptor.0.descriptor_id();

        let wallet_keychain = ((KeychainKind::Other(descriptor_id)), public_descriptor);

        self.keychains.push(wallet_keychain);
        self
    }

    pub fn list_keychains(&self) -> &Vec<WalletKeychain> {
        &self.keychains
    }

    pub fn list_keychain_ids(&self) -> Vec<DescriptorId> {
        self.keychains
            .iter()
            .map(|keychain| match keychain.0 {
                KeychainKind::Other(descriptor_id) => descriptor_id,
                KeychainKind::Default => keychain.1.0.descriptor_id(),
                KeychainKind::Change => keychain.1.0.descriptor_id(),
            })
            .collect()
    }

    // pub fn add_change_keychain(&mut self, keychain: (DescriptorToExtract, KeyMap), keychain_identifier: KeychainIdentifier) {
    //
    // }
    // pub fn add_wallet_keychain<D: IntoWalletDescriptor + Send + 'static>(
    //     &mut self,
    //     descriptor: D,
    // )
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
    /// properly maintain the feerate when adding this input to a transaction during coin selection.
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

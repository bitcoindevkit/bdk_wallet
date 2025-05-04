// Bitcoin Dev Kit
// Written in 2020 by Alekos Filini <alekos.filini@gmail.com>
//
// Copyright (c) 2020-2025 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use std::prelude::rust_2021::Vec;
use bitcoin::Network;
use chain::{DescriptorExt, DescriptorId};
use miniscript::{Descriptor, DescriptorPublicKey};
use miniscript::descriptor::KeyMap;
use crate::descriptor::IntoWalletDescriptor;
use crate::{DescriptorToExtract, KeychainKind};
use crate::wallet::make_descriptor_to_extract;
use crate::wallet::utils::SecpCtx;

/// A `WalletKeychain` is mostly a descriptor with metadata associated with it. It states whether the
/// keychain is the default keychain for the wallet, and provides an identifier for it which can be
/// used for retrieval.
pub type WalletKeychain = (KeychainKind, (Descriptor<DescriptorPublicKey>, KeyMap));

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
        let wallet_keychain = (KeychainKind::Default, public_descriptor);

        KeyRing {
            keychains: vec![wallet_keychain],
            network,
        }
    }

    // TODO #226: This needs to never fail because there is always a default keychain.
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
}

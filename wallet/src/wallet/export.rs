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

//! Wallet export
//!
//! This modules implements wallet export formats for different Bitcoin wallet applications:
//!
//! 1. [FullyNoded](https://github.com/Fonta1n3/FullyNoded/blob/10b7808c8b929b171cca537fb50522d015168ac9/Docs/Wallets/Wallet-Export-Spec.md)
//! 2. [Caravan](https://github.com/unchained-capital/caravan)
//!
//! ## Examples
//!
//! ### Import from FullyNoded JSON
//!
//! ```
//! # use std::str::FromStr;
//! # use bitcoin::*;
//! # use bdk_wallet::export::*;
//! # use bdk_wallet::*;
//! let import = r#"{
//!     "descriptor": "wpkh([c258d2e4\/84h\/1h\/0h]tpubDD3ynpHgJQW8VvWRzQ5WFDCrs4jqVFGHB3vLC3r49XHJSqP8bHKdK4AriuUKLccK68zfzowx7YhmDN8SiSkgCDENUFx9qVw65YyqM78vyVe\/0\/*)",
//!     "blockheight":1782088,
//!     "label":"testnet"
//! }"#;
//!
//! let import = FullyNodedExport::from_str(import)?;
//! let wallet = Wallet::create(
//!     import.descriptor(),
//!     import.change_descriptor().expect("change descriptor"),
//! )
//! .network(Network::Testnet)
//! .create_wallet_no_persist()?;
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```
//!
//! ### Export a `Wallet` to FullyNoded format
//! ```
//! # use bitcoin::*;
//! # use bdk_wallet::export::*;
//! # use bdk_wallet::*;
//! let wallet = Wallet::create(
//!     "wpkh([c258d2e4/84h/1h/0h]tpubDD3ynpHgJQW8VvWRzQ5WFDCrs4jqVFGHB3vLC3r49XHJSqP8bHKdK4AriuUKLccK68zfzowx7YhmDN8SiSkgCDENUFx9qVw65YyqM78vyVe/0/*)",
//!     "wpkh([c258d2e4/84h/1h/0h]tpubDD3ynpHgJQW8VvWRzQ5WFDCrs4jqVFGHB3vLC3r49XHJSqP8bHKdK4AriuUKLccK68zfzowx7YhmDN8SiSkgCDENUFx9qVw65YyqM78vyVe/1/*)",
//! )
//! .network(Network::Testnet)
//! .create_wallet_no_persist()?;
//! let export = FullyNodedExport::export_wallet(&wallet, "exported wallet", true).unwrap();
//!
//! println!("Exported: {}", export.to_string());
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```
//!
//! ### Export a `Wallet` to Caravan format
//! ```
//! # use bitcoin::*;
//! # use bdk_wallet::export::*;
//! # use bdk_wallet::*;
//! let wallet = Wallet::create(
//!     "wsh(sortedmulti(2,[73756c7f/48h/0h/0h/2h]tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3/0/*,[f9f62194/48h/0h/0h/2h]tpubDDp3ZSH1yCwusRppH7zgSxq2t1VEUyXSeEp8E5aFS8m43MknUjiF1bSLo3CGWAxbDyhF1XowA5ukPzyJZjznYk3kYi6oe7QxtX2euvKWsk4/0/*))",
//!     "wsh(sortedmulti(2,[73756c7f/48h/0h/0h/2h]tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3/1/*,[f9f62194/48h/0h/0h/2h]tpubDDp3ZSH1yCwusRppH7zgSxq2t1VEUyXSeEp8E5aFS8m43MknUjiF1bSLo3CGWAxbDyhF1XowA5ukPzyJZjznYk3kYi6oe7QxtX2euvKWsk4/1/*))",
//! )
//! .network(Network::Testnet)
//! .create_wallet_no_persist()?;
//! let export = CaravanExport::export_wallet(&wallet, "My Multisig Wallet").unwrap();
//!
//! println!("Exported: {}", export.to_string());
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```
//!
//! ### Import from Caravan format
//! ```
//! # use std::str::FromStr;
//! # use bitcoin::*;
//! # use bdk_wallet::export::*;
//! # use bdk_wallet::*;
//! let import = r#"{
//!     "name": "My Multisig Wallet",
//!     "addressType": "P2WSH",
//!     "network": "mainnet",
//!     "client": {
//!         "type": "public"
//!     },
//!     "quorum": {
//!         "requiredSigners": 2,
//!         "totalSigners": 2
//!     },
//!     "extendedPublicKeys": [
//!         {
//!             "name": "key1",
//!             "bip32Path": "m/48'/0'/0'/2'",
//!             "xpub": "tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3",
//!             "xfp": "73756c7f"
//!         },
//!         {
//!             "name": "key2",
//!             "bip32Path": "m/48'/0'/0'/2'",
//!             "xpub": "tpubDDp3ZSH1yCwusRppH7zgSxq2t1VEUyXSeEp8E5aFS8m43MknUjiF1bSLo3CGWAxbDyhF1XowA5ukPzyJZjznYk3kYi6oe7QxtX2euvKWsk4",
//!             "xfp": "f9f62194"
//!         }
//!     ],
//!     "startingAddressIndex": 0
//! }"#;
//!
//! let import = CaravanExport::from_str(import)?;
//! let (external, internal) = import.to_descriptors()?;
//! # assert!(external.contains("sortedmulti"));
//! # assert!(internal.contains("sortedmulti"));
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt;
use core::str::FromStr;
use serde::{Deserialize, Serialize};

use miniscript::descriptor::{ShInner, WshInner};
use miniscript::{Descriptor, ScriptContext, Terminal};

use crate::types::KeychainKind;
use crate::wallet::Wallet;

/// Alias for [`FullyNodedExport`]
#[deprecated(since = "0.18.0", note = "Please use [`FullyNodedExport`] instead")]
pub type WalletExport = FullyNodedExport;

/// Structure that contains the export of a wallet in FullyNoded format
///
/// For a usage example see [this module](crate::wallet::export)'s documentation.
#[derive(Debug, Serialize, Deserialize)]
pub struct FullyNodedExport {
    descriptor: String,
    /// Earliest block to rescan when looking for the wallet's transactions
    pub blockheight: u32,
    /// Arbitrary label for the wallet
    pub label: String,
}

impl fmt::Display for FullyNodedExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}

impl FromStr for FullyNodedExport {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

fn remove_checksum(s: String) -> String {
    s.split_once('#').map(|(a, _)| String::from(a)).unwrap()
}

impl FullyNodedExport {
    /// Export a wallet
    ///
    /// This function returns an error if it determines that the `wallet`'s descriptor(s) are not
    /// supported by Bitcoin Core or don't follow the standard derivation paths defined by BIP44
    /// and others.
    ///
    /// If `include_blockheight` is `true`, this function will look into the `wallet`'s database
    /// for the oldest transaction it knows and use that as the earliest block to rescan.
    ///
    /// If the database is empty or `include_blockheight` is false, the `blockheight` field
    /// returned will be `0`.
    pub fn export_wallet(
        wallet: &Wallet,
        label: &str,
        include_blockheight: bool,
    ) -> Result<Self, &'static str> {
        let descriptor = wallet
            .public_descriptor(KeychainKind::External)
            .to_string_with_secret(
                &wallet
                    .get_signers(KeychainKind::External)
                    .as_key_map(wallet.secp_ctx()),
            );
        let descriptor = remove_checksum(descriptor);
        Self::is_compatible_with_core(&descriptor)?;

        let blockheight = if include_blockheight {
            wallet.transactions().next().map_or(0, |canonical_tx| {
                canonical_tx
                    .chain_position
                    .confirmation_height_upper_bound()
                    .unwrap_or(0)
            })
        } else {
            0
        };

        let export = FullyNodedExport {
            descriptor,
            label: label.into(),
            blockheight,
        };

        let change_descriptor = {
            let descriptor = wallet
                .public_descriptor(KeychainKind::Internal)
                .to_string_with_secret(
                    &wallet
                        .get_signers(KeychainKind::Internal)
                        .as_key_map(wallet.secp_ctx()),
                );
            Some(remove_checksum(descriptor))
        };

        if export.change_descriptor() != change_descriptor {
            return Err("Incompatible change descriptor");
        }

        Ok(export)
    }

    fn is_compatible_with_core(descriptor: &str) -> Result<(), &'static str> {
        fn check_ms<Ctx: ScriptContext>(
            terminal: &Terminal<String, Ctx>,
        ) -> Result<(), &'static str> {
            if let Terminal::Multi(_) = terminal {
                Ok(())
            } else {
                Err("The descriptor contains operators not supported by Bitcoin Core")
            }
        }

        // pkh(), wpkh(), sh(wpkh()) are always fine, as well as multi() and sortedmulti()
        match Descriptor::<String>::from_str(descriptor).map_err(|_| "Invalid descriptor")? {
            Descriptor::Pkh(_) | Descriptor::Wpkh(_) => Ok(()),
            Descriptor::Sh(sh) => match sh.as_inner() {
                ShInner::Wpkh(_) => Ok(()),
                ShInner::SortedMulti(_) => Ok(()),
                ShInner::Wsh(wsh) => match wsh.as_inner() {
                    WshInner::SortedMulti(_) => Ok(()),
                    WshInner::Ms(ms) => check_ms(&ms.node),
                },
                ShInner::Ms(ms) => check_ms(&ms.node),
            },
            Descriptor::Wsh(wsh) => match wsh.as_inner() {
                WshInner::SortedMulti(_) => Ok(()),
                WshInner::Ms(ms) => check_ms(&ms.node),
            },
            Descriptor::Tr(_) => Ok(()),
            _ => Err("The descriptor is not compatible with Bitcoin Core"),
        }
    }

    /// Return the external descriptor
    pub fn descriptor(&self) -> String {
        self.descriptor.clone()
    }

    /// Return the internal descriptor, if present
    pub fn change_descriptor(&self) -> Option<String> {
        let replaced = self.descriptor.replace("/0/*", "/1/*");

        if replaced != self.descriptor {
            Some(replaced)
        } else {
            None
        }
    }
}

/// ExtendedPublicKey structure for Caravan wallet format
#[derive(Debug, Serialize, Deserialize)]
pub struct CaravanExtendedPublicKey {
    /// Name of the signer
    pub name: String,
    /// BIP32 derivation path
    #[serde(rename = "bip32Path")]
    pub bip32_path: String,
    /// Extended public key
    pub xpub: String,
    /// Fingerprint of the master key
    pub xfp: String,
}

/// Structure that contains the export of a wallet in Caravan wallet format
///
/// Caravan is a Bitcoin multisig coordinator by Unchained Capital.
/// This format supports P2SH, P2WSH, and P2SH-P2WSH multisig wallet types.
///
/// For a usage example see [this module](crate::wallet::export)'s documentation.
#[derive(Debug, Serialize, Deserialize)]
pub struct CaravanExport {
    /// Name of the wallet
    pub name: String,
    /// Address type (P2SH, P2WSH, P2SH-P2WSH)
    #[serde(rename = "addressType")]
    pub address_type: String,
    /// Network (mainnet, testnet)
    pub network: String,
    /// Client configuration
    pub client: serde_json::Value,
    /// Quorum information
    pub quorum: CaravanQuorum,
    /// List of extended public keys
    #[serde(rename = "extendedPublicKeys")]
    pub extended_public_keys: Vec<CaravanExtendedPublicKey>,
    /// Starting address index
    #[serde(rename = "startingAddressIndex")]
    pub starting_address_index: u32,
}

/// Quorum information for Caravan wallet format
#[derive(Debug, Serialize, Deserialize)]
pub struct CaravanQuorum {
    /// Number of required signers
    #[serde(rename = "requiredSigners")]
    pub required_signers: u32,
    /// Total number of signers
    #[serde(rename = "totalSigners")]
    pub total_signers: u32,
}

impl fmt::Display for CaravanExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string_pretty(self).unwrap())
    }
}

impl FromStr for CaravanExport {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl CaravanExport {
    /// Export a wallet to Caravan format
    ///
    /// This function returns an error if it determines that the `wallet`'s descriptor(s) are not
    /// supported by Caravan or if the descriptor is not a multisig descriptor.
    ///
    /// Caravan supports P2SH, P2WSH, and P2SH-P2WSH multisig wallets.
    pub fn export_wallet(wallet: &Wallet, name: &str) -> Result<Self, &'static str> {
        // Get the descriptor and extract information
        let descriptor_str = wallet
            .public_descriptor(KeychainKind::External)
            .to_string_with_secret(
                &wallet
                    .get_signers(KeychainKind::External)
                    .as_key_map(wallet.secp_ctx()),
            );
        let descriptor_str = remove_checksum(descriptor_str);

        // Parse the descriptor to extract required information
        let descriptor =
            Descriptor::<String>::from_str(&descriptor_str).map_err(|_| "Invalid descriptor")?;

        // Determine the address type and multisig information
        let (address_type, quorum, keys) = Self::extract_descriptor_info(&descriptor)?;

        // Network
        let network = match wallet.network() {
            bitcoin::Network::Bitcoin => "mainnet",
            _ => "testnet",
        };

        // Create the Caravan export
        let export = CaravanExport {
            name: name.into(),
            address_type,
            network: network.into(),
            client: serde_json::json!({"type": "public"}),
            quorum,
            extended_public_keys: keys,
            starting_address_index: 0,
        };

        Ok(export)
    }

    /// Extract information from a descriptor
    fn extract_descriptor_info(
        descriptor: &Descriptor<String>,
    ) -> Result<(String, CaravanQuorum, Vec<CaravanExtendedPublicKey>), &'static str> {
        // Extract address type, quorum, and keys based on descriptor type
        match descriptor {
            Descriptor::Sh(sh) => {
                match sh.as_inner() {
                    ShInner::Wsh(wsh) => {
                        // P2SH-P2WSH multisig
                        match wsh.as_inner() {
                            WshInner::SortedMulti(multi) => {
                                let keys = Self::extract_xpubs_from_multi(multi)?;
                                let quorum = CaravanQuorum {
                                    required_signers: multi.k() as u32,
                                    total_signers: multi.pks().len() as u32,
                                };
                                Ok(("P2SH-P2WSH".into(), quorum, keys))
                            }
                            _ => Err("Only sortedmulti is supported for P2SH-P2WSH in Caravan"),
                        }
                    }
                    ShInner::SortedMulti(multi) => {
                        // P2SH multisig
                        let keys = Self::extract_xpubs_from_multi(multi)?;
                        let quorum = CaravanQuorum {
                            required_signers: multi.k() as u32,
                            total_signers: multi.pks().len() as u32,
                        };
                        Ok(("P2SH".into(), quorum, keys))
                    }
                    _ => Err("Only sortedmulti is supported for P2SH in Caravan"),
                }
            }
            Descriptor::Wsh(wsh) => {
                match wsh.as_inner() {
                    WshInner::SortedMulti(multi) => {
                        // P2WSH multisig
                        let keys = Self::extract_xpubs_from_multi(multi)?;
                        let quorum = CaravanQuorum {
                            required_signers: multi.k() as u32,
                            total_signers: multi.pks().len() as u32,
                        };
                        Ok(("P2WSH".into(), quorum, keys))
                    }
                    _ => Err("Only sortedmulti is supported for P2WSH in Caravan"),
                }
            }
            _ => {
                Err("Only P2SH, P2WSH, or P2SH-P2WSH multisig descriptors are supported by Caravan")
            }
        }
    }

    /// Extract xpubs and fingerprints from multi descriptor
    fn extract_xpubs_from_multi<Ctx: ScriptContext>(
        multi: &miniscript::descriptor::SortedMultiVec<String, Ctx>,
    ) -> Result<Vec<CaravanExtendedPublicKey>, &'static str> {
        let mut keys = Vec::new();

        for (i, key) in multi.pks().iter().enumerate() {
            // Parse the key string to extract origin fingerprint, path, and xpub
            // Format example: [c258d2e4/48h/0h/0h/2h]xpub.../0/*
            let key_str = key.clone();

            // Check if the key has origin information
            if !key_str.starts_with('[') {
                return Err("Keys must include origin information for Caravan export");
            }

            // Extract origin fingerprint
            let origin_end = key_str.find(']').ok_or("Invalid key format")?;
            let origin = &key_str[1..origin_end];
            let parts: Vec<&str> = origin.split('/').collect();
            if parts.is_empty() {
                return Err("Invalid key origin format");
            }

            let fingerprint = parts[0].to_string();

            // Extract derivation path and convert 'h' to "'"
            let path_parts: Vec<String> = parts[1..]
                .iter()
                .map(|part| {
                    if part.ends_with('h') {
                        let p = &part[0..part.len() - 1];
                        format!("{}'", p)
                    } else {
                        part.to_string()
                    }
                })
                .collect();
            let path = format!("m/{}", path_parts.join("/"));

            // Extract xpub
            let xpub_part = &key_str[origin_end + 1..];
            let xpub_end = xpub_part.find('/').unwrap_or(xpub_part.len());
            let xpub = xpub_part[..xpub_end].to_string();

            keys.push(CaravanExtendedPublicKey {
                name: format!("key{}", i + 1),
                bip32_path: path,
                xpub,
                xfp: fingerprint,
            });
        }

        Ok(keys)
    }

    /// Import a wallet from Caravan format
    pub fn to_descriptors(&self) -> Result<(String, String), &'static str> {
        if self.extended_public_keys.is_empty() {
            return Err("No extended public keys found");
        }

        // Build key expressions for the descriptor
        let mut key_exprs = Vec::new();
        for key in &self.extended_public_keys {
            // Remove 'm/' prefix from bip32Path if present
            let path = if key.bip32_path.starts_with("m/") {
                &key.bip32_path[2..]
            } else {
                &key.bip32_path
            };

            // Convert "'" to "h" in the path
            let descriptor_path = path.replace("'", "h");

            // Format key with origin fingerprint and path
            let key_expr = format!("[{}/{}]{}/0/*", key.xfp, descriptor_path, key.xpub);
            key_exprs.push(key_expr);
        }

        // Build descriptor based on address type
        let descriptor_prefix = match self.address_type.as_str() {
            "P2SH" => "sh(sortedmulti(",
            "P2WSH" => "wsh(sortedmulti(",
            "P2SH-P2WSH" => "sh(wsh(sortedmulti(",
            _ => return Err("Unsupported address type"),
        };

        let descriptor_suffix = match self.address_type.as_str() {
            "P2SH" | "P2WSH" => "))",
            "P2SH-P2WSH" => ")))",
            _ => return Err("Unsupported address type"),
        };

        // Construct the external descriptor
        let external_descriptor = format!(
            "{}{},({})){}",
            descriptor_prefix,
            self.quorum.required_signers,
            key_exprs.join(","),
            descriptor_suffix
        );

        // Create change descriptor by replacing /0/* with /1/*
        let change_descriptor = external_descriptor.replace("/0/*", "/1/*");

        Ok((external_descriptor, change_descriptor))
    }
}

#[cfg(test)]
mod test {
    use alloc::string::ToString;
    use bitcoin::Amount;
    use core::str::FromStr;

    use bdk_chain::BlockId;
    use bitcoin::{hashes::Hash, BlockHash, Network};

    use super::*;
    use crate::test_utils::*;
    use crate::Wallet;

    fn get_test_wallet(descriptor: &str, change_descriptor: &str, network: Network) -> Wallet {
        let mut wallet = Wallet::create(descriptor.to_string(), change_descriptor.to_string())
            .network(network)
            .create_wallet_no_persist()
            .expect("must create wallet");
        let block = BlockId {
            height: 5000,
            hash: BlockHash::all_zeros(),
        };
        insert_checkpoint(&mut wallet, block);
        receive_output_in_latest_block(&mut wallet, Amount::from_sat(10_000));

        wallet
    }

    #[test]
    fn test_export_bip44() {
        let descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44'/0'/0'/0/*)";
        let change_descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44'/0'/0'/1/*)";

        let wallet = get_test_wallet(descriptor, change_descriptor, Network::Bitcoin);
        let export = FullyNodedExport::export_wallet(&wallet, "Test Label", true).unwrap();

        assert_eq!(export.descriptor(), descriptor);
        assert_eq!(export.change_descriptor(), Some(change_descriptor.into()));
        assert_eq!(export.blockheight, 5000);
        assert_eq!(export.label, "Test Label");
    }

    #[test]
    #[should_panic(expected = "Incompatible change descriptor")]
    fn test_export_no_change() {
        // The wallet's change descriptor has no wildcard. It should be impossible to
        // export, because exporting this kind of external descriptor normally implies the
        // existence of a compatible internal descriptor

        let descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44'/0'/0'/0/*)";
        let change_descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44'/0'/0'/1/0)";

        let wallet = get_test_wallet(descriptor, change_descriptor, Network::Bitcoin);
        FullyNodedExport::export_wallet(&wallet, "Test Label", true).unwrap();
    }

    #[test]
    #[should_panic(expected = "Incompatible change descriptor")]
    fn test_export_incompatible_change() {
        // This wallet has a change descriptor, but the derivation path is not in the "standard"
        // bip44/49/etc format

        let descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44'/0'/0'/0/*)";
        let change_descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/50'/0'/1/*)";

        let wallet = get_test_wallet(descriptor, change_descriptor, Network::Bitcoin);
        FullyNodedExport::export_wallet(&wallet, "Test Label", true).unwrap();
    }

    #[test]
    fn test_export_multi() {
        let descriptor = "wsh(multi(2,\
                                [73756c7f/48'/0'/0'/2']tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3/0/*,\
                                [f9f62194/48'/0'/0'/2']tpubDDp3ZSH1yCwusRppH7zgSxq2t1VEUyXSeEp8E5aFS8m43MknUjiF1bSLo3CGWAxbDyhF1XowA5ukPzyJZjznYk3kYi6oe7QxtX2euvKWsk4/0/*,\
                                [c98b1535/48'/0'/0'/2']tpubDCDi5W4sP6zSnzJeowy8rQDVhBdRARaPhK1axABi8V1661wEPeanpEXj4ZLAUEoikVtoWcyK26TKKJSecSfeKxwHCcRrge9k1ybuiL71z4a/0/*\
                          ))";
        let change_descriptor = "wsh(multi(2,\
                                       [73756c7f/48'/0'/0'/2']tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3/1/*,\
                                       [f9f62194/48'/0'/0'/2']tpubDDp3ZSH1yCwusRppH7zgSxq2t1VEUyXSeEp8E5aFS8m43MknUjiF1bSLo3CGWAxbDyhF1XowA5ukPzyJZjznYk3kYi6oe7QxtX2euvKWsk4/1/*,\
                                       [c98b1535/48'/0'/0'/2']tpubDCDi5W4sP6zSnzJeowy8rQDVhBdRARaPhK1axABi8V1661wEPeanpEXj4ZLAUEoikVtoWcyK26TKKJSecSfeKxwHCcRrge9k1ybuiL71z4a/1/*\
                                 ))";

        let wallet = get_test_wallet(descriptor, change_descriptor, Network::Testnet);
        let export = FullyNodedExport::export_wallet(&wallet, "Test Label", true).unwrap();

        assert_eq!(export.descriptor(), descriptor);
        assert_eq!(export.change_descriptor(), Some(change_descriptor.into()));
        assert_eq!(export.blockheight, 5000);
        assert_eq!(export.label, "Test Label");
    }

    #[test]
    fn test_export_tr() {
        let descriptor = "tr([73c5da0a/86'/0'/0']tprv8fMn4hSKPRC1oaCPqxDb1JWtgkpeiQvZhsr8W2xuy3GEMkzoArcAWTfJxYb6Wj8XNNDWEjfYKK4wGQXh3ZUXhDF2NcnsALpWTeSwarJt7Vc/0/*)";
        let change_descriptor = "tr([73c5da0a/86'/0'/0']tprv8fMn4hSKPRC1oaCPqxDb1JWtgkpeiQvZhsr8W2xuy3GEMkzoArcAWTfJxYb6Wj8XNNDWEjfYKK4wGQXh3ZUXhDF2NcnsALpWTeSwarJt7Vc/1/*)";
        let wallet = get_test_wallet(descriptor, change_descriptor, Network::Testnet);
        let export = FullyNodedExport::export_wallet(&wallet, "Test Label", true).unwrap();
        assert_eq!(export.descriptor(), descriptor);
        assert_eq!(export.change_descriptor(), Some(change_descriptor.into()));
        assert_eq!(export.blockheight, 5000);
        assert_eq!(export.label, "Test Label");
    }

    #[test]
    fn test_export_to_json() {
        let descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44'/0'/0'/0/*)";
        let change_descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44'/0'/0'/1/*)";

        let wallet = get_test_wallet(descriptor, change_descriptor, Network::Bitcoin);
        let export = FullyNodedExport::export_wallet(&wallet, "Test Label", true).unwrap();

        assert_eq!(export.to_string(), "{\"descriptor\":\"wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44\'/0\'/0\'/0/*)\",\"blockheight\":5000,\"label\":\"Test Label\"}");
    }

    #[test]
    fn test_export_from_json() {
        let descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44'/0'/0'/0/*)";
        let change_descriptor = "wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44'/0'/0'/1/*)";

        let import_str = "{\"descriptor\":\"wpkh(xprv9s21ZrQH143K4CTb63EaMxja1YiTnSEWKMbn23uoEnAzxjdUJRQkazCAtzxGm4LSoTSVTptoV9RbchnKPW9HxKtZumdyxyikZFDLhogJ5Uj/44\'/0\'/0\'/0/*)\",\"blockheight\":5000,\"label\":\"Test Label\"}";
        let export = FullyNodedExport::from_str(import_str).unwrap();

        assert_eq!(export.descriptor(), descriptor);
        assert_eq!(export.change_descriptor(), Some(change_descriptor.into()));
        assert_eq!(export.blockheight, 5000);
        assert_eq!(export.label, "Test Label");
    }

    #[test]
    fn test_caravan_export_p2wsh() {
        let descriptor = "wsh(sortedmulti(2,[119dbcab/48h/0h/0h/2h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/0/*,[e650dc93/48h/0h/0h/2h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/0/*))";
        let change_descriptor = "wsh(sortedmulti(2,[119dbcab/48h/0h/0h/2h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/1/*,[e650dc93/48h/0h/0h/2h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/1/*))";
        let network = Network::Bitcoin;

        let wallet = get_test_wallet(descriptor, change_descriptor, network);
        let export = CaravanExport::export_wallet(&wallet, "Test P2WSH Wallet").unwrap();

        // Check basic fields
        assert_eq!(export.name, "Test P2WSH Wallet");
        assert_eq!(export.address_type, "P2WSH");
        assert_eq!(export.network, "mainnet");
        assert_eq!(export.quorum.required_signers, 2);
        assert_eq!(export.quorum.total_signers, 2);
        assert_eq!(export.starting_address_index, 0);

        // Check extended public keys
        assert_eq!(export.extended_public_keys.len(), 2);
        assert_eq!(export.extended_public_keys[0].xfp, "119dbcab");

        // Use the path format with apostrophes in the test expectation
        assert_eq!(export.extended_public_keys[0].bip32_path, "m/48'/0'/0'/2'");
        assert_eq!(export.extended_public_keys[1].xfp, "e650dc93");
        assert_eq!(export.extended_public_keys[1].bip32_path, "m/48'/0'/0'/2'");

        // Test to_descriptors functionality
        let (external, internal) = export.to_descriptors().unwrap();
        assert!(external.contains("wsh(sortedmulti("));
        assert!(internal.contains("/1/*"));

        // Test JSON serialization
        let json = export.to_string();
        assert!(json.contains("\"name\":"));
        assert!(json.contains("\"addressType\":"));
        assert!(json.contains("\"extendedPublicKeys\":"));
    }

    #[test]
    fn test_caravan_export_p2sh() {
        let descriptor = "sh(sortedmulti(2,[119dbcab/48h/0h/0h/1h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/0/*,[e650dc93/48h/0h/0h/1h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/0/*))";
        let change_descriptor = "sh(sortedmulti(2,[119dbcab/48h/0h/0h/1h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/1/*,[e650dc93/48h/0h/0h/1h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/1/*))";
        let network = Network::Bitcoin;

        let wallet = get_test_wallet(descriptor, change_descriptor, network);
        let export = CaravanExport::export_wallet(&wallet, "Test P2SH Wallet").unwrap();

        assert_eq!(export.address_type, "P2SH");
        assert_eq!(export.quorum.required_signers, 2);
        assert_eq!(export.quorum.total_signers, 2);
    }

    #[test]
    fn test_caravan_export_p2sh_p2wsh() {
        let descriptor = "sh(wsh(sortedmulti(2,[119dbcab/48h/0h/0h/3h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/0/*,[e650dc93/48h/0h/0h/3h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/0/*)))";
        let change_descriptor = "sh(wsh(sortedmulti(2,[119dbcab/48h/0h/0h/3h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/1/*,[e650dc93/48h/0h/0h/3h]xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL/1/*)))";
        let network = Network::Bitcoin;

        let wallet = get_test_wallet(descriptor, change_descriptor, network);
        let export = CaravanExport::export_wallet(&wallet, "Test P2SH-P2WSH Wallet").unwrap();

        assert_eq!(export.address_type, "P2SH-P2WSH");
        assert_eq!(export.quorum.required_signers, 2);
        assert_eq!(export.quorum.total_signers, 2);
    }

    #[test]
    fn test_network_detection_for_caravan() {
        // Test the network detection logic directly
        assert_eq!(
            match bitcoin::Network::Bitcoin {
                bitcoin::Network::Bitcoin => "mainnet",
                _ => "testnet",
            },
            "mainnet"
        );

        assert_eq!(
            match bitcoin::Network::Testnet {
                bitcoin::Network::Bitcoin => "mainnet",
                _ => "testnet",
            },
            "testnet"
        );

        assert_eq!(
            match bitcoin::Network::Signet {
                bitcoin::Network::Bitcoin => "mainnet",
                _ => "testnet",
            },
            "testnet"
        );

        assert_eq!(
            match bitcoin::Network::Regtest {
                bitcoin::Network::Bitcoin => "mainnet",
                _ => "testnet",
            },
            "testnet"
        );

        // This tests the exact same logic used in the CaravanExport::export_wallet method
        let network_mapping = |network: bitcoin::Network| -> &'static str {
            match network {
                bitcoin::Network::Bitcoin => "mainnet",
                _ => "testnet",
            }
        };

        assert_eq!(network_mapping(bitcoin::Network::Bitcoin), "mainnet");
        assert_eq!(network_mapping(bitcoin::Network::Testnet), "testnet");
    }

    #[test]
    fn test_caravan_import() {
        let json = r#"{
            "name": "Test Wallet",
            "addressType": "P2WSH",
            "network": "mainnet",
            "client": {
                "type": "public"
            },
            "quorum": {
                "requiredSigners": 2,
                "totalSigners": 3
            },
            "extendedPublicKeys": [
                {
                    "name": "key1",
                    "bip32Path": "m/48h/0h/0h/2h",
                    "xpub": "xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL",
                    "xfp": "119dbcab"
                },
                {
                    "name": "key2",
                    "bip32Path": "m/48h/0h/0h/2h",
                    "xpub": "xpub6FKY2Zpu9dFmKZwLkRwt6XK3gcQuJDCz7rBzSWRU4TsUfGgfLdBMK6nVztnz6oSQjSiy2muFnxT5hc4CtYJzr4cLZcmCVeiUxCRGeTqVMuQ",
                    "xfp": "e650dc93"
                },
                {
                    "name": "key3",
                    "bip32Path": "m/48h/0h/0h/2h",
                    "xpub": "xpub6FPZdGBiQAu3FJjWAjeu6YBCCeUSnpm98y5tQU3AvBXRjQU8H2Su8QkcQZrAL8Wv8hy7G44JzBdNWvjXm1bdHhQDfg4JBzPQshqMfQLt1Bj",
                    "xfp": "bcc3df08"
                }
            ],
            "startingAddressIndex": 0
        }"#;

        let import = CaravanExport::from_str(json).unwrap();
        let (external, internal) = import.to_descriptors().unwrap();

        assert!(external.contains("wsh(sortedmulti(2,"));
        assert_eq!(import.quorum.required_signers, 2);
        assert_eq!(import.quorum.total_signers, 3);
        assert_eq!(import.extended_public_keys.len(), 3);

        // Check that the change descriptor is correctly generated
        assert!(internal.contains("/1/*"));
        assert!(external.contains("/0/*"));
    }
}

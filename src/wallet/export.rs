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
//! # assert_eq!(external, "wsh(sortedmulti(2,[73756c7f/48'/0'/0'/2']tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3/0/*,[f9f62194/48'/0'/0'/2']tpubDDp3ZSH1yCwusRppH7zgSxq2t1VEUyXSeEp8E5aFS8m43MknUjiF1bSLo3CGWAxbDyhF1XowA5ukPzyJZjznYk3kYi6oe7QxtX2euvKWsk4/0/*))#pgthjwtg");
//! # assert_eq!(internal, "wsh(sortedmulti(2,[73756c7f/48'/0'/0'/2']tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3/1/*,[f9f62194/48'/0'/0'/2']tpubDDp3ZSH1yCwusRppH7zgSxq2t1VEUyXSeEp8E5aFS8m43MknUjiF1bSLo3CGWAxbDyhF1XowA5ukPzyJZjznYk3kYi6oe7QxtX2euvKWsk4/1/*))#cmcnua7a");
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Important Notes on Import/Export Operations
//!
//! ### One-Way Operations
//!
//! The Caravan export/import functionality is designed for **one-way operations**:
//!
//! 1. **Export**: `Wallet → CaravanExport` (for sharing with Caravan)
//! 2. **Import**: `JSON → CaravanExport → to_descriptors` (for importing from Caravan)
//!
//! ### Round-Trip Limitation
//!
//! **Note**: Full round-trip operations (`Descriptor → Wallet → CaravanExport → to_descriptors`)
//! will **not** produce identical descriptors due to how derivation paths are processed for
//! Caravan compatibility. This is expected behavior and not a bug.
//!
//! If you need to preserve the exact original descriptor format, store it separately
//! rather than relying on round-trip conversion.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use bitcoin::bip32::{DerivationPath, Fingerprint, Xpub};
use core::fmt;
use core::str::FromStr;
use serde::{Deserialize, Serialize};

use miniscript::descriptor::{DescriptorPublicKey, ShInner, WshInner};
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
#[derive(Debug, Clone)]
pub struct CaravanExtendedPublicKey {
    /// Name of the signer
    pub name: String,
    /// BIP32 derivation path
    /// This contains the combined path (origin path + derivation path)
    /// Note: In the future, we might consider separating this into:
    /// - origin_path: for the path from master to xpub
    /// - derivation_path: for the additional derivation steps in the descriptor
    ///
    /// But for now, Caravan expects a single combined path.
    pub bip32_path: DerivationPath,
    /// Extended public key
    pub xpub: Xpub,
    /// Fingerprint of the master key
    pub xfp: Fingerprint,
}

// Custom serde implementation to maintain JSON compatibility
impl Serialize for CaravanExtendedPublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("CaravanExtendedPublicKey", 4)?;
        state.serialize_field("name", &self.name)?;
        // Add m/ prefix for JSON compatibility
        let path_with_prefix = format!("m/{}", self.bip32_path);
        state.serialize_field("bip32Path", &path_with_prefix)?;
        state.serialize_field("xpub", &self.xpub.to_string())?;
        state.serialize_field("xfp", &self.xfp.to_string())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for CaravanExtendedPublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "camelCase")]
        enum Field {
            Name,
            Bip32Path,
            Xpub,
            Xfp,
        }

        struct CaravanKeyVisitor;

        impl<'de> Visitor<'de> for CaravanKeyVisitor {
            type Value = CaravanExtendedPublicKey;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct CaravanExtendedPublicKey")
            }

            fn visit_map<V>(self, mut map: V) -> Result<CaravanExtendedPublicKey, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut name = None;
                let mut bip32_path = None;
                let mut xpub = None;
                let mut xfp = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Name => {
                            if name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value()?);
                        }
                        Field::Bip32Path => {
                            if bip32_path.is_some() {
                                return Err(de::Error::duplicate_field("bip32Path"));
                            }
                            let path_str: String = map.next_value()?;
                            // DerivationPath can handle m/ prefix natively
                            bip32_path = Some(
                                DerivationPath::from_str(&path_str).map_err(de::Error::custom)?,
                            );
                        }
                        Field::Xpub => {
                            if xpub.is_some() {
                                return Err(de::Error::duplicate_field("xpub"));
                            }
                            let xpub_str: String = map.next_value()?;
                            xpub = Some(Xpub::from_str(&xpub_str).map_err(de::Error::custom)?);
                        }
                        Field::Xfp => {
                            if xfp.is_some() {
                                return Err(de::Error::duplicate_field("xfp"));
                            }
                            let xfp_str: String = map.next_value()?;
                            xfp = Some(Fingerprint::from_str(&xfp_str).map_err(de::Error::custom)?);
                        }
                    }
                }

                Ok(CaravanExtendedPublicKey {
                    name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                    bip32_path: bip32_path.ok_or_else(|| de::Error::missing_field("bip32Path"))?,
                    xpub: xpub.ok_or_else(|| de::Error::missing_field("xpub"))?,
                    xfp: xfp.ok_or_else(|| de::Error::missing_field("xfp"))?,
                })
            }
        }

        const FIELDS: &[&str] = &["name", "bip32Path", "xpub", "xfp"];
        deserializer.deserialize_struct("CaravanExtendedPublicKey", FIELDS, CaravanKeyVisitor)
    }
}

/// Address type for Caravan wallet format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CaravanAddressType {
    /// P2SH multisig
    #[serde(rename = "P2SH")]
    P2SH,
    /// P2WSH multisig (native SegWit)
    #[serde(rename = "P2WSH")]
    P2WSH,
    /// P2SH-P2WSH multisig (nested SegWit)
    #[serde(rename = "P2SH-P2WSH")]
    P2SHWrappedP2WSH,
}

/// Caravan network.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaravanNetwork {
    /// mainnet
    Mainnet,
    /// testnet
    Testnet,
}

use bitcoin::NetworkKind;

impl From<bitcoin::Network> for CaravanNetwork {
    fn from(network: bitcoin::Network) -> Self {
        match network.into() {
            NetworkKind::Main => Self::Mainnet,
            NetworkKind::Test => Self::Testnet,
        }
    }
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
    pub address_type: CaravanAddressType,
    /// Network (mainnet, testnet)
    pub network: CaravanNetwork,
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
        // Get the descriptor directly from the wallet
        let descriptor = wallet.public_descriptor(KeychainKind::External);

        // Determine the address type and multisig information
        let (address_type, quorum, keys) = Self::extract_descriptor_info(descriptor)?;

        // Create the Caravan export
        let export = CaravanExport {
            name: name.into(),
            address_type,
            network: wallet.network().into(),
            client: serde_json::json!({"type": "public"}),
            quorum,
            extended_public_keys: keys,
            starting_address_index: 0,
        };

        Ok(export)
    }

    /// Extract information from a descriptor
    fn extract_descriptor_info(
        descriptor: &Descriptor<DescriptorPublicKey>,
    ) -> Result<
        (
            CaravanAddressType,
            CaravanQuorum,
            Vec<CaravanExtendedPublicKey>,
        ),
        &'static str,
    > {
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
                                Ok((CaravanAddressType::P2SHWrappedP2WSH, quorum, keys))
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
                        Ok((CaravanAddressType::P2SH, quorum, keys))
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
                        Ok((CaravanAddressType::P2WSH, quorum, keys))
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
        multi: &miniscript::descriptor::SortedMultiVec<DescriptorPublicKey, Ctx>,
    ) -> Result<Vec<CaravanExtendedPublicKey>, &'static str> {
        let mut keys = Vec::new();

        for (i, desc_key) in multi.pks().iter().enumerate() {
            match desc_key {
                DescriptorPublicKey::XPub(xpub_key) => {
                    // Extract fingerprint from origin or derive from xpub
                    let fingerprint = match &xpub_key.origin {
                        Some((fp, _)) => *fp,
                        None => {
                            // If no origin, we need the secp context to derive fingerprint
                            // This should ideally be passed as a parameter
                            return Err("Missing origin information for key");
                        }
                    };

                    // Extract the base derivation path (without the final /0/* or /1/*)
                    // Caravan expects the complete derivation path excluding the final step
                    // This means combining the origin path with the descriptor's derivation path
                    // but excluding the last step (which will be added by Caravan later)
                    let base_path = if !xpub_key.derivation_path.is_empty() {
                        // Remove the final derivation step (0 or 1) to get the base path
                        // Convert to vector, remove last element, then back to DerivationPath
                        let base_vec: Vec<_> = xpub_key
                            .derivation_path
                            .into_iter()
                            .take(xpub_key.derivation_path.len() - 1)
                            .cloned()
                            .collect();
                        let base: DerivationPath = base_vec.into();

                        // Combine with origin path if present, as Caravan expects the complete path
                        // from master to just before the final step
                        match &xpub_key.origin {
                            Some((_, origin_path)) => origin_path.extend(&base),
                            None => base,
                        }
                    } else {
                        match &xpub_key.origin {
                            Some((_, origin_path)) => origin_path.clone(),
                            None => DerivationPath::default(),
                        }
                    };

                    keys.push(CaravanExtendedPublicKey {
                        name: format!("key{}", i + 1),
                        bip32_path: base_path,
                        xpub: xpub_key.xkey,
                        xfp: fingerprint,
                    });
                }
                _ => return Err("Only extended public keys are supported"),
            }
        }

        Ok(keys)
    }

    /// Import a wallet from Caravan format using proper Descriptor construction
    pub fn to_descriptors(&self) -> Result<(String, String), &'static str> {
        if self.extended_public_keys.is_empty() {
            return Err("No extended public keys found");
        }

        // Create DescriptorPublicKey objects for external and internal chains
        let external_keys: Result<Vec<DescriptorPublicKey>, _> = self
            .extended_public_keys
            .iter()
            .map(|key| {
                let key_str = format!("[{}/{}]{}/0/*", key.xfp, key.bip32_path, key.xpub);
                DescriptorPublicKey::from_str(&key_str)
                    .map_err(|_| "Failed to create DescriptorPublicKey")
            })
            .collect();
        let external_keys = external_keys?;

        let internal_keys: Result<Vec<DescriptorPublicKey>, _> = self
            .extended_public_keys
            .iter()
            .map(|key| {
                let key_str = format!("[{}/{}]{}/1/*", key.xfp, key.bip32_path, key.xpub);
                DescriptorPublicKey::from_str(&key_str)
                    .map_err(|_| "Failed to create DescriptorPublicKey")
            })
            .collect();
        let internal_keys = internal_keys?;

        let k = self.quorum.required_signers as usize;

        // Use proper Descriptor construction methods
        let external_desc: Descriptor<DescriptorPublicKey> = match self.address_type {
            CaravanAddressType::P2SH => Descriptor::new_sh_sortedmulti(k, external_keys)
                .map_err(|_| "Failed to create P2SH sortedmulti descriptor")?,
            CaravanAddressType::P2WSH => Descriptor::new_wsh_sortedmulti(k, external_keys)
                .map_err(|_| "Failed to create P2WSH sortedmulti descriptor")?,
            CaravanAddressType::P2SHWrappedP2WSH => {
                Descriptor::new_sh_wsh_sortedmulti(k, external_keys)
                    .map_err(|_| "Failed to create P2SH-P2WSH sortedmulti descriptor")?
            }
        };

        let internal_desc: Descriptor<DescriptorPublicKey> = match self.address_type {
            CaravanAddressType::P2SH => Descriptor::new_sh_sortedmulti(k, internal_keys)
                .map_err(|_| "Failed to create P2SH sortedmulti descriptor")?,
            CaravanAddressType::P2WSH => Descriptor::new_wsh_sortedmulti(k, internal_keys)
                .map_err(|_| "Failed to create P2WSH sortedmulti descriptor")?,
            CaravanAddressType::P2SHWrappedP2WSH => {
                Descriptor::new_sh_wsh_sortedmulti(k, internal_keys)
                    .map_err(|_| "Failed to create P2SH-P2WSH sortedmulti descriptor")?
            }
        };

        Ok((external_desc.to_string(), internal_desc.to_string()))
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
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
        assert_eq!(export.address_type, CaravanAddressType::P2WSH);
        assert_eq!(export.network, CaravanNetwork::Mainnet);
        assert_eq!(export.quorum.required_signers, 2);
        assert_eq!(export.quorum.total_signers, 2);
        assert_eq!(export.starting_address_index, 0);

        // Check extended public keys
        assert_eq!(export.extended_public_keys.len(), 2);
        assert_eq!(
            export.extended_public_keys[0].xfp,
            Fingerprint::from_str("119dbcab").unwrap()
        );

        // Use the path format with apostrophes in the test expectation
        assert_eq!(
            export.extended_public_keys[0].bip32_path,
            DerivationPath::from_str("m/48'/0'/0'/2'").unwrap()
        );
        assert_eq!(
            export.extended_public_keys[1].xfp,
            Fingerprint::from_str("e650dc93").unwrap()
        );
        assert_eq!(
            export.extended_public_keys[1].bip32_path,
            DerivationPath::from_str("m/48'/0'/0'/2'").unwrap()
        );

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

        assert_eq!(export.address_type, CaravanAddressType::P2SH);
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

        assert_eq!(export.address_type, CaravanAddressType::P2SHWrappedP2WSH);
        assert_eq!(export.quorum.required_signers, 2);
        assert_eq!(export.quorum.total_signers, 2);
    }

    #[test]
    fn test_caravan_network_conversion() {
        // Test CaravanNetwork enum with From<bitcoin::Network> implementation
        assert_eq!(
            serde_json::to_string(&CaravanNetwork::from(bitcoin::Network::Bitcoin)).unwrap(),
            "\"mainnet\""
        );

        assert_eq!(
            serde_json::to_string(&CaravanNetwork::from(bitcoin::Network::Testnet)).unwrap(),
            "\"testnet\""
        );

        assert_eq!(
            serde_json::to_string(&CaravanNetwork::from(bitcoin::Network::Signet)).unwrap(),
            "\"testnet\""
        );

        assert_eq!(
            serde_json::to_string(&CaravanNetwork::from(bitcoin::Network::Regtest)).unwrap(),
            "\"testnet\""
        );
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
                    "bip32Path": "m/48'/0'/0'/2'",
                    "xpub": "xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL",
                    "xfp": "119dbcab"
                },
                {
                    "name": "key2",
                    "bip32Path": "m/48'/0'/0'/2'",
                    "xpub": "xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL",
                    "xfp": "e650dc93"
                },
                {
                    "name": "key3",
                    "bip32Path": "m/48'/0'/0'/2'",
                    "xpub": "xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL",
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

    #[test]
    fn test_caravan_descriptors_create_functional_wallet() {
        // Test that resulting descriptor strings can create a functional BDK Wallet
        let json = r#"{
            "name": "Test Wallet",
            "addressType": "P2WSH",
            "network": "testnet",
            "client": {
                "type": "public"
            },
            "quorum": {
                "requiredSigners": 2,
                "totalSigners": 2
            },
            "extendedPublicKeys": [
                {
                    "name": "key1",
                    "bip32Path": "m/48'/1'/0'/2'",
                    "xpub": "tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3",
                    "xfp": "73756c7f"
                },
                {
                    "name": "key2",
                    "bip32Path": "m/48'/1'/0'/2'",
                    "xpub": "tpubDDp3ZSH1yCwusRppH7zgSxq2t1VEUyXSeEp8E5aFS8m43MknUjiF1bSLo3CGWAxbDyhF1XowA5ukPzyJZjznYk3kYi6oe7QxtX2euvKWsk4",
                    "xfp": "f9f62194"
                }
            ],
            "startingAddressIndex": 0
        }"#;

        let import = CaravanExport::from_str(json).unwrap();
        let (external_desc, internal_desc) = import.to_descriptors().unwrap();

        // Verify the descriptors can create a functional BDK Wallet
        let wallet_result = Wallet::create(external_desc, internal_desc)
            .network(bitcoin::Network::Testnet)
            .create_wallet_no_persist();

        assert!(
            wallet_result.is_ok(),
            "Failed to create wallet from Caravan export descriptors: {:?}",
            wallet_result.err()
        );

        let mut wallet = wallet_result.unwrap();

        // Verify basic wallet functionality
        assert_eq!(wallet.network(), bitcoin::Network::Testnet);

        // Test address generation to verify the wallet is functional
        let address = wallet.reveal_next_address(crate::types::KeychainKind::External);
        assert_eq!(address.index, 0);

        // Verify it's a proper script hash address (P2WSH)
        let addr_str = address.address.to_string();
        assert!(
            addr_str.starts_with("tb1"),
            "Expected testnet bech32 address, got: {}",
            addr_str
        );
    }

    #[test]
    fn test_extract_xpubs_complex_derivation() {
        // Create a descriptor with complex derivation paths (multiple steps)
        // This simulates a case with [fingerprint/path]xpub/additional/path
        let descriptor_str = "wsh(sortedmulti(2,[3f3b5353/48'/0'/0'/2']tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3/0/0,[f9f62194/48'/0'/0'/2']tpubDDp3ZSH1yCwusRppH7zgSxq2t1VEUyXSeEp8E5aFS8m43MknUjiF1bSLo3CGWAxbDyhF1XowA5ukPzyJZjznYk3kYi6oe7QxtX2euvKWsk4/0/0))";
        let descriptor = Descriptor::<DescriptorPublicKey>::from_str(descriptor_str).unwrap();

        // Get the multi object from the descriptor
        let multi = match descriptor {
            Descriptor::Wsh(wsh) => {
                if let miniscript::descriptor::WshInner::SortedMulti(multi) = wsh.into_inner() {
                    multi
                } else {
                    panic!("Expected a SortedMulti in the descriptor")
                }
            }
            _ => panic!("Expected a WSH descriptor"),
        };

        // Extract the keys
        let keys = CaravanExport::extract_xpubs_from_multi(&multi).unwrap();

        // Verify we have 2 keys
        assert_eq!(keys.len(), 2);

        // Check the first key
        let key1 = &keys[0];
        assert_eq!(key1.name, "key1");
        assert_eq!(key1.xfp, Fingerprint::from_str("3f3b5353").unwrap());
        assert_eq!(
            key1.bip32_path,
            DerivationPath::from_str("m/48'/0'/0'/2'/0").unwrap(),
            "First key should have combined path from origin and derivation"
        );

        // Check the second key
        let key2 = &keys[1];
        assert_eq!(key2.name, "key2");
        assert_eq!(key2.xfp, Fingerprint::from_str("f9f62194").unwrap());
        assert_eq!(
            key2.bip32_path,
            DerivationPath::from_str("m/48'/0'/0'/2'/0").unwrap(),
            "Second key should have combined path from origin and derivation"
        );
    }

    #[test]
    fn test_caravan_extended_public_key_serialize_deserialize() {
        let key = CaravanExtendedPublicKey {
            name: "test_key".to_string(),
            bip32_path: DerivationPath::from_str("m/48'/0'/0'/2'/0").unwrap(),
            xpub: bitcoin::bip32::Xpub::from_str("tpubDCKxNyM3bLgbEX13Mcd8mYxbVg9ajDkWXMh29hMWBurKfVmBfWAM96QVP3zaUcN51HvkZ3ar4VwP82kC8JZhhux8vFQoJintSpVBwpFvyU3").unwrap(),
            xfp: Fingerprint::from_str("3f3b5353").unwrap(),
        };

        // Serialize to JSON
        let json = serde_json::to_string(&key).unwrap();

        // Deserialize from JSON
        let deserialized: CaravanExtendedPublicKey = serde_json::from_str(&json).unwrap();

        // Verify fields match
        assert_eq!(key.name, deserialized.name);
        assert_eq!(key.bip32_path, deserialized.bip32_path);
        assert_eq!(key.xpub.to_string(), deserialized.xpub.to_string());
        assert_eq!(key.xfp, deserialized.xfp);
    }
}

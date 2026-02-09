// Bitcoin Dev Kit
// Written in 2026 by Aaliyah Junaid <junaidaaliyah260@gmail.com>
//
// Copyright (c) 2020-2026 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

//! BIP-0329 Wallet Labels
//!
//! This module implements the [BIP-0329] wallet labels export format, which defines
//! a standard way to export and import labels for transactions, addresses, public keys,
//! inputs, outputs, and extended public keys.
//!
//! [BIP-0329]: https://github.com/bitcoin/bips/blob/master/bip-0329.mediawiki

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use bitcoin::hashes::{sha256, Hash};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Maximum label length in characters. Labels longer than this will be truncated.
pub const MAX_LABEL_LENGTH: usize = 255;

/// BIP-0329 label types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelType {
    /// Transaction label
    Tx,
    /// Address label
    Addr,
    /// Public key label
    Pubkey,
    /// Transaction input label (references OutPoint being spent)
    Input,
    /// Transaction output label
    Output,
    /// Extended public key label
    Xpub,
}

impl fmt::Display for LabelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LabelType::Tx => write!(f, "tx"),
            LabelType::Addr => write!(f, "addr"),
            LabelType::Pubkey => write!(f, "pubkey"),
            LabelType::Input => write!(f, "input"),
            LabelType::Output => write!(f, "output"),
            LabelType::Xpub => write!(f, "xpub"),
        }
    }
}

/// A BIP-0329 label record.
///
/// This enum represents the different types of labels that can be exported/imported.
/// Each variant contains type-specific optional fields as defined in BIP-0329.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LabelRecord {
    /// Transaction label with optional metadata
    Tx(TxLabel),
    /// Address label with optional metadata
    Addr(AddrLabel),
    /// Public key label
    Pubkey(PubkeyLabel),
    /// Transaction input label with optional metadata
    Input(InputLabel),
    /// Transaction output label with optional metadata
    Output(OutputLabel),
    /// Extended public key label
    Xpub(XpubLabel),
}

impl LabelRecord {
    /// Get the label type.
    pub fn label_type(&self) -> LabelType {
        match self {
            LabelRecord::Tx(_) => LabelType::Tx,
            LabelRecord::Addr(_) => LabelType::Addr,
            LabelRecord::Pubkey(_) => LabelType::Pubkey,
            LabelRecord::Input(_) => LabelType::Input,
            LabelRecord::Output(_) => LabelType::Output,
            LabelRecord::Xpub(_) => LabelType::Xpub,
        }
    }

    /// Get the reference string (txid, address, outpoint, etc.).
    pub fn reference(&self) -> &str {
        match self {
            LabelRecord::Tx(l) => &l.r#ref,
            LabelRecord::Addr(l) => &l.r#ref,
            LabelRecord::Pubkey(l) => &l.r#ref,
            LabelRecord::Input(l) => &l.r#ref,
            LabelRecord::Output(l) => &l.r#ref,
            LabelRecord::Xpub(l) => &l.r#ref,
        }
    }

    /// Get the label text.
    pub fn label(&self) -> &str {
        match self {
            LabelRecord::Tx(l) => &l.label,
            LabelRecord::Addr(l) => &l.label,
            LabelRecord::Pubkey(l) => &l.label,
            LabelRecord::Input(l) => &l.label,
            LabelRecord::Output(l) => &l.label,
            LabelRecord::Xpub(l) => &l.label,
        }
    }

    /// Truncate the label if it exceeds the maximum length.
    /// Returns a warning if truncation occurred.
    pub fn truncate_if_needed(&mut self) -> Option<LabelWarning> {
        fn truncate_string(s: &mut String) -> Option<LabelWarning> {
            let len = s.chars().count();
            if len > MAX_LABEL_LENGTH {
                *s = s.chars().take(MAX_LABEL_LENGTH).collect();
                Some(LabelWarning {
                    original_length: len,
                    truncated_to: MAX_LABEL_LENGTH,
                })
            } else {
                None
            }
        }

        match self {
            LabelRecord::Tx(l) => truncate_string(&mut l.label),
            LabelRecord::Addr(l) => truncate_string(&mut l.label),
            LabelRecord::Pubkey(l) => truncate_string(&mut l.label),
            LabelRecord::Input(l) => truncate_string(&mut l.label),
            LabelRecord::Output(l) => truncate_string(&mut l.label),
            LabelRecord::Xpub(l) => truncate_string(&mut l.label),
        }
    }
}

/// Transaction label with optional BIP-0329 metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxLabel {
    /// Transaction ID (hex string)
    #[serde(rename = "ref")]
    pub r#ref: String,
    /// Label text (max 255 characters)
    pub label: String,
    /// Descriptor origin (e.g., `wpkh([d34db33f/84'/0'/0'])`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Block height (omit or 0 if < 6 confirmations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// ISO-8601 timestamp of the block
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// Transaction fee in satoshis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<i64>,
    /// Net value change in satoshis (positive = inflow, negative = outflow)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
    /// Exchange rate at time of transaction (currency code -> rate)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<BTreeMap<String, f64>>,
}

impl TxLabel {
    /// Create a new transaction label.
    pub fn new(txid: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            r#ref: txid.into(),
            label: truncate_label(label.into()),
            origin: None,
            height: None,
            time: None,
            fee: None,
            value: None,
            rate: None,
        }
    }
}

/// Address label with optional BIP-0329 metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AddrLabel {
    /// Address string
    #[serde(rename = "ref")]
    pub r#ref: String,
    /// Label text (max 255 characters)
    pub label: String,
    /// Keypath (e.g., `/1/123` or full descriptor)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keypath: Option<String>,
    /// Block heights with activity for this address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heights: Option<Vec<u32>>,
}

impl AddrLabel {
    /// Create a new address label.
    pub fn new(address: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            r#ref: address.into(),
            label: truncate_label(label.into()),
            keypath: None,
            heights: None,
        }
    }
}

/// Public key label.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PubkeyLabel {
    /// Public key (hex string)
    #[serde(rename = "ref")]
    pub r#ref: String,
    /// Label text (max 255 characters)
    pub label: String,
}

impl PubkeyLabel {
    /// Create a new public key label.
    pub fn new(pubkey: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            r#ref: pubkey.into(),
            label: truncate_label(label.into()),
        }
    }
}

/// Transaction input label with optional BIP-0329 metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputLabel {
    /// OutPoint reference (`txid:vout`)
    #[serde(rename = "ref")]
    pub r#ref: String,
    /// Label text (max 255 characters)
    pub label: String,
    /// Keypath (e.g., `/1/123` or full descriptor)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keypath: Option<String>,
    /// Value in satoshis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
    /// Fair market value (currency code -> value)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fmv: Option<BTreeMap<String, f64>>,
    /// Block height
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// ISO-8601 timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
}

impl InputLabel {
    /// Create a new input label from outpoint string.
    pub fn new(outpoint: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            r#ref: outpoint.into(),
            label: truncate_label(label.into()),
            keypath: None,
            value: None,
            fmv: None,
            height: None,
            time: None,
        }
    }
}

/// Transaction output label with optional BIP-0329 metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputLabel {
    /// OutPoint reference (`txid:vout`)
    #[serde(rename = "ref")]
    pub r#ref: String,
    /// Label text (max 255 characters)
    pub label: String,
    /// Whether the output is spendable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spendable: Option<bool>,
    /// Keypath (e.g., `/1/123` or full descriptor)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keypath: Option<String>,
    /// Value in satoshis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
    /// Fair market value (currency code -> value)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fmv: Option<BTreeMap<String, f64>>,
    /// Block height
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// ISO-8601 timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
}

impl OutputLabel {
    /// Create a new output label from outpoint string.
    pub fn new(outpoint: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            r#ref: outpoint.into(),
            label: truncate_label(label.into()),
            spendable: None,
            keypath: None,
            value: None,
            fmv: None,
            height: None,
            time: None,
        }
    }
}

/// Extended public key label.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct XpubLabel {
    /// Extended public key string
    #[serde(rename = "ref")]
    pub r#ref: String,
    /// Label text (max 255 characters)
    pub label: String,
}

impl XpubLabel {
    /// Create a new xpub label.
    pub fn new(xpub: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            r#ref: xpub.into(),
            label: truncate_label(label.into()),
        }
    }
}

/// Warning returned when a label operation results in truncation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelWarning {
    /// Original label length before truncation
    pub original_length: usize,
    /// Length after truncation
    pub truncated_to: usize,
}

impl fmt::Display for LabelWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Label truncated from {} to {} characters",
            self.original_length, self.truncated_to
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for LabelWarning {}

/// Truncate a label to the maximum length.
fn truncate_label(label: String) -> String {
    if label.chars().count() <= MAX_LABEL_LENGTH {
        label
    } else {
        label.chars().take(MAX_LABEL_LENGTH).collect()
    }
}

/// Check if a label would be truncated and return a warning if so.
pub fn check_label_length(label: &str) -> Option<LabelWarning> {
    let len = label.chars().count();
    if len > MAX_LABEL_LENGTH {
        Some(LabelWarning {
            original_length: len,
            truncated_to: MAX_LABEL_LENGTH,
        })
    } else {
        None
    }
}

/// Key for storing labels in the changeset.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LabelKey {
    /// The type of label
    pub label_type: LabelType,
    /// The reference (txid, address, outpoint, etc.)
    pub reference: String,
}

impl LabelKey {
    /// Create a new label key.
    pub fn new(label_type: LabelType, reference: impl Into<String>) -> Self {
        Self {
            label_type,
            reference: reference.into(),
        }
    }

    /// Create a label key for a transaction.
    pub fn for_tx(txid: bitcoin::Txid) -> Self {
        use alloc::string::ToString;
        Self::new(LabelType::Tx, txid.to_string())
    }

    /// Create a label key for an address.
    pub fn for_addr(address: &bitcoin::Address) -> Self {
        use alloc::string::ToString;
        Self::new(LabelType::Addr, address.to_string())
    }

    /// Create a label key for an input (spent OutPoint).
    pub fn for_input(outpoint: bitcoin::OutPoint) -> Self {
        use alloc::string::ToString;
        Self::new(LabelType::Input, outpoint.to_string())
    }

    /// Create a label key for an output (OutPoint).
    pub fn for_output(outpoint: bitcoin::OutPoint) -> Self {
        use alloc::string::ToString;
        Self::new(LabelType::Output, outpoint.to_string())
    }

    /// Create a label key for a public key.
    pub fn for_pubkey(pubkey: &bitcoin::PublicKey) -> Self {
        use alloc::string::ToString;
        Self::new(LabelType::Pubkey, pubkey.to_string())
    }

    /// Create a label key for an extended public key.
    pub fn for_xpub(xpub: &bitcoin::bip32::Xpub) -> Self {
        use alloc::string::ToString;
        Self::new(LabelType::Xpub, xpub.to_string())
    }
}

/// Labels changeset for persistence.
#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChangeSet {
    /// Map of label keys to label records. `None` values indicate deletion.
    pub labels: BTreeMap<LabelKey, Option<LabelRecord>>,
}

impl bdk_chain::Merge for ChangeSet {
    fn merge(&mut self, other: Self) {
        for (key, value) in other.labels {
            self.labels.insert(key, value);
        }
    }

    fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

// ============ Import/Export Support ============

/// Error that can occur during label import.
#[derive(Clone, Debug)]
pub enum LabelImportError {
    /// Failed to parse JSON line
    JsonParsing {
        /// The line number that failed to parse
        line: usize,
        /// The error message
        message: String,
    },
    /// Missing required field
    MissingField {
        /// The line number with the missing field
        line: usize,
        /// The name of the missing field
        field: &'static str,
    },
    /// Unknown label type
    UnknownType {
        /// The line number with unknown type
        line: usize,
        /// The type that was encountered
        type_name: String,
    },
    /// Encryption error
    Encryption(LabelEncryptionError),
}

impl fmt::Display for LabelImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LabelImportError::JsonParsing { line, message } => {
                write!(f, "Line {}: JSON parsing error: {}", line, message)
            }
            LabelImportError::MissingField { line, field } => {
                write!(f, "Line {}: missing required field '{}'", line, field)
            }
            LabelImportError::UnknownType { line, type_name } => {
                write!(f, "Line {}: unknown label type '{}'", line, type_name)
            }
            LabelImportError::Encryption(e) => {
                write!(f, "Encryption error: {}", e)
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for LabelImportError {}

/// Result of importing labels.
#[derive(Clone, Debug, Default)]
pub struct LabelImportResult {
    /// Successfully imported labels
    pub labels: Vec<LabelRecord>,
    /// Errors encountered during import
    pub errors: Vec<LabelImportError>,
    /// Warnings (e.g., truncated labels)
    pub warnings: Vec<LabelWarning>,
}

/// Export labels to BIP-0329 JSONL format (optionally encrypted).
///
/// Returns a vector of bytes. If `passphrase` is provided, the output is encrypted.
/// Otherwise, it contains the JSONL string bytes.
pub fn export_labels<'a>(
    labels: impl Iterator<Item = &'a LabelRecord>,
    passphrase: Option<&str>,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut output = String::new();
    for record in labels {
        let line = serde_json::to_string(record)?;
        output.push_str(&line);
        output.push('\n');
    }

    if let Some(pass) = passphrase {
        Ok(encrypt(output.as_bytes(), pass))
    } else {
        Ok(output.into_bytes())
    }
}

/// Import labels from BIP-0329 JSONL format (optionally decrypted).
///
/// Parses `data` (which may be encrypted if `passphrase` is provided) and returns the parsed
/// labels. Invalid lines are collected as errors but do not stop the import process.
pub fn import_labels(data: &[u8], passphrase: Option<&str>) -> LabelImportResult {
    use alloc::string::ToString;

    let mut result = LabelImportResult::default();

    let decrypted_data = if let Some(pass) = passphrase {
        match decrypt(data, pass) {
            Ok(d) => d,
            Err(e) => {
                result.errors.push(LabelImportError::Encryption(e));
                return result;
            }
        }
    } else {
        data.to_vec()
    };

    let jsonl = match String::from_utf8(decrypted_data) {
        Ok(s) => s,
        Err(e) => {
            result.errors.push(LabelImportError::JsonParsing {
                line: 0,
                message: format!("Invalid UTF-8: {}", e),
            });
            return result;
        }
    };

    for (line_num, line) in jsonl.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<LabelRecord>(line) {
            Ok(mut record) => {
                // Check for truncation and collect warning
                if let Some(warning) = record.truncate_if_needed() {
                    result.warnings.push(warning);
                }
                result.labels.push(record);
            }
            Err(e) => {
                result.errors.push(LabelImportError::JsonParsing {
                    line: line_num + 1,
                    message: e.to_string(),
                });
            }
        }
    }

    result
}

/// Error during label encryption or decryption.
#[derive(Clone, Debug)]
pub enum LabelEncryptionError {
    /// Decryption failed (invalid passphrase or corrupted data)
    DecryptionFailed,
    /// Invalid data format (too short for nonce)
    InvalidFormat,
}

impl fmt::Display for LabelEncryptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LabelEncryptionError::DecryptionFailed => write!(f, "Decryption failed"),
            LabelEncryptionError::InvalidFormat => write!(f, "Invalid data format"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for LabelEncryptionError {}

/// Encrypt data using ChaCha20Poly1305 with a key derived from the passphrase.
///
/// The key is SHA256(passphrase). A random 12-byte nonce is generated and prepended to the
/// ciphertext.
pub fn encrypt(data: &[u8], passphrase: &str) -> Vec<u8> {
    let key_hash = sha256::Hash::hash(passphrase.as_bytes());
    let key = key_hash.as_byte_array();
    let cipher = ChaCha20Poly1305::new_from_slice(key).expect("Key size is correct");

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .expect("Encryption should succeed");

    let mut result = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend(ciphertext);
    result
}

/// Decrypt data using ChaCha20Poly1305 with a key derived from the passphrase.
///
/// Expects data to be `nonce (12 bytes) || ciphertext`.
pub fn decrypt(data: &[u8], passphrase: &str) -> Result<Vec<u8>, LabelEncryptionError> {
    if data.len() < 12 {
        return Err(LabelEncryptionError::InvalidFormat);
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key_hash = sha256::Hash::hash(passphrase.as_bytes());
    let key = key_hash.as_byte_array();
    let cipher = ChaCha20Poly1305::new_from_slice(key).expect("Key size is correct");

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| LabelEncryptionError::DecryptionFailed)
}

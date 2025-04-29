//! Example demonstrating how to use bdk_wallet with an external BIP-329 library
//! for managing wallet labels persisted in a separate file.

extern crate anyhow;
extern crate bdk_wallet;
extern crate bip329;
extern crate bitcoin;
extern crate env_logger;
extern crate log;
extern crate tempfile;

use anyhow::{anyhow, Context, Result};
use bdk_wallet::{
    descriptor,
    keys::bip39::{Language, Mnemonic, WordCount},
    keys::GeneratableKey,
    keys::{DerivableKey, DescriptorSecretKey, ExtendedKey, GeneratedKey},
    miniscript::{self, Descriptor, DescriptorPublicKey},
    AddressInfo,
    KeychainKind,
    Wallet, // Added AddressInfo
};
use bip329::{AddressRecord, Label, LabelRef, Labels, TransactionRecord};
use bitcoin::{address::NetworkUnchecked, bip32::DerivationPath, Address, Network, OutPoint, Txid};
use log::{error, info, warn};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::ErrorKind,
    ops::DerefMut,
    // Removed path::PathBuf - no longer needed explicitly
    str::FromStr,
};
use tempfile::tempdir;

// --- Helper Functions ---
fn format_ref_str(item_ref: &LabelRef) -> String {
    // Use the existing Display impl from bip329::LabelRef which handles assume_checked()
    match item_ref {
        LabelRef::Txid(txid) => format!("txid:{}", txid),
        LabelRef::Address(_) => format!("addr:{}", item_ref), // Rely on LabelRef's Display
        LabelRef::Output(op) => format!("output:{}", op),
        LabelRef::Input(op) => format!("input:{}", op),
        LabelRef::PublicKey(pk_str) => format!("pubkey:{}", pk_str),
        LabelRef::Xpub(xpub_str) => format!("xpub:{}", xpub_str),
    }
}
fn format_bdk_addr_ref(addr: &Address) -> String {
    format!("addr:{}", addr)
}
fn format_bdk_txid_ref(txid: Txid) -> String {
    format!("txid:{}", txid)
}
fn format_bdk_outpoint_ref(op: OutPoint) -> String {
    format!("output:{}", op)
}

// --- Main Example Logic ---
fn main() -> Result<()> {
    // Initialize logger - Output controlled by RUST_LOG env var
    env_logger::init();

    info!("--- BDK Wallet + BIP-329 Label Example ---");

    let temp_dir = tempdir().context("Failed to create temporary directory")?;
    let label_file_path = temp_dir.path().join("bdk_bip329_example_labels.jsonl"); // PathBuf inferred
    info!("Using temporary label file: {}", label_file_path.display());

    let network = Network::Regtest;

    // 1. Generate Keys and Descriptors Programmatically
    info!("Generating keys and descriptors...");
    let mnemonic: GeneratedKey<_, miniscript::Segwitv0> =
        Mnemonic::generate((WordCount::Words12, Language::English))
            .map_err(|_| anyhow!("Mnemonic generation failed"))?;
    let mnemonic_words = mnemonic.to_string();
    info!("Generated Mnemonic: {}", mnemonic_words);
    let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_words)?;
    let xkey: ExtendedKey = mnemonic.into_extended_key()?;
    let master_xprv = xkey
        .into_xprv(network)
        .ok_or_else(|| anyhow!("Could not derive xprv for network {}", network))?;
    let external_path = DerivationPath::from_str("m/84h/1h/0h/0")?;
    let internal_path = DerivationPath::from_str("m/84h/1h/0h/1")?;

    let (external_descriptor, _ext_keymap, _ext_networks): (
        Descriptor<DescriptorPublicKey>,
        BTreeMap<DescriptorPublicKey, DescriptorSecretKey>,
        HashSet<Network>,
    ) = descriptor!(wpkh((master_xprv.clone(), external_path)))?;

    let (internal_descriptor, _int_keymap, _int_networks): (
        Descriptor<DescriptorPublicKey>,
        BTreeMap<DescriptorPublicKey, DescriptorSecretKey>,
        HashSet<Network>,
    ) = descriptor!(wpkh((master_xprv, internal_path)))?;

    let external_descriptor_str = external_descriptor.to_string();
    let internal_descriptor_str = internal_descriptor.to_string();

    info!("External Descriptor: {}", external_descriptor_str);
    info!("Internal Descriptor: {}", internal_descriptor_str);

    // 2. Create the BDK Wallet (Corrected: Pass owned Strings, removed mut)
    let wallet = Wallet::create(external_descriptor_str, internal_descriptor_str) // Pass String, not &String, removed mut
        .network(network)
        .create_wallet_no_persist()
        .context("Failed to create wallet using generated descriptors")?;
    info!("Wallet created successfully.");

    // Get example items using peek_address
    let address0: AddressInfo = wallet.peek_address(KeychainKind::External, 0);
    let address1: AddressInfo = wallet.peek_address(KeychainKind::External, 1);

    let dummy_txid =
        Txid::from_str("f4184fc596403b9d638783cf57adfe4c75c605f6356fbc91338530e9831e9e16")?;
    let dummy_outpoint = OutPoint::new(dummy_txid, 0);
    info!(
        "Wallet Addresses: Index {} -> {}",
        address0.index, address0.address
    );
    info!(
        "                  Index {} -> {}",
        address1.index, address1.address
    );
    info!("Dummy TXID: {}", dummy_txid);
    info!("Dummy OutPoint: {}", dummy_outpoint);

    // 3. Load Labels from temporary file (or create empty)
    info!("\n--- Loading Labels ---");
    // Pass label_file_path by reference
    let mut labels = match Labels::try_from_file(&label_file_path) {
        Ok(loaded_labels) => {
            info!(
                "Loaded {} labels from temporary file '{}'.",
                loaded_labels.len(),
                label_file_path.display()
            );
            loaded_labels
        }
        Err(bip329::error::ParseError::FileReadError(io_err))
            if io_err.kind() == ErrorKind::NotFound =>
        {
            info!(
                "Temporary label file '{}' not found, starting empty.",
                label_file_path.display()
            );
            Labels::default()
        }
        Err(e) => {
            return Err(anyhow!(
                "Failed to load labels from {}: {}",
                label_file_path.display(),
                e
            ));
        }
    };

    // Build lookup map
    let mut label_lookup: HashMap<String, String> = HashMap::new();
    for label_entry in labels.iter() {
        if let Some(label_text) = label_entry.label() {
            let ref_str = format_ref_str(&label_entry.ref_());
            label_lookup.insert(ref_str, label_text.to_string());
        }
    }

    // 4. Correlate Wallet Data with Labels
    info!("\n--- Current Labels for Wallet Items ---");
    // Use address0 and address1 correctly
    let items_to_lookup: Vec<(&str, String)> = vec![
        ("Address 0", format_bdk_addr_ref(&address0.address)), // Use address0
        ("Address 1", format_bdk_addr_ref(&address1.address)), // Use address1
        ("Dummy Tx", format_bdk_txid_ref(dummy_txid)),
        ("Dummy UTXO", format_bdk_outpoint_ref(dummy_outpoint)),
    ];
    for (item_desc, item_ref_str) in &items_to_lookup {
        match label_lookup.get(item_ref_str) {
            Some(label_text) => info!("{} ({}): {}", item_desc, item_ref_str, label_text),
            None => info!("{} ({}): [No Label]", item_desc, item_ref_str),
        }
    }

    // 5. Add/Update Labels in Memory
    info!("\n--- Adding/Updating Labels ---");
    // Use address0 for the label update logic
    let addr0_ref_str = format_bdk_addr_ref(&address0.address);
    let new_addr0_label = "Primary Receiving Address (Index 0)";
    let labels_vec = labels.deref_mut();
    match labels_vec
        .iter_mut()
        .find(|l| format_ref_str(&l.ref_()) == addr0_ref_str) // Use addr0_ref_str
    {
        Some(label_entry) => {
            info!("Updating label for {}", addr0_ref_str);
            match label_entry {
                Label::Address(record) => record.label = Some(new_addr0_label.to_string()), // Use new_addr0_label
                _ => warn!(
                    "Warning: Found ref string {} but not Address label?",
                    addr0_ref_str
                ),
            }
        }
        None => {
            info!("Adding new label for {}", addr0_ref_str);
            // Use address0 for conversion
            let addr_unchecked: Address<NetworkUnchecked> =
                Address::from_str(&address0.address.to_string())?.into_unchecked();
            labels_vec.push(Label::Address(AddressRecord {
                ref_: addr_unchecked,
                label: Some(new_addr0_label.to_string()), // Use new_addr0_label
            }));
        }
    }
    let tx_ref_str = format_bdk_txid_ref(dummy_txid);
    if !labels_vec
        .iter()
        .any(|l| format_ref_str(&l.ref_()) == tx_ref_str)
    {
        info!("Adding new label for {}", tx_ref_str);
        labels_vec.push(Label::Transaction(TransactionRecord {
            ref_: dummy_txid,
            label: Some("Simulated Incoming TX".to_string()),
            origin: None,
        }));
    }

    // 6. Export and Save Labels to temporary file
    info!("\n--- Exporting and Saving Labels ---");
    match labels.export_to_file(&label_file_path) {
        // Pass &PathBuf ref
        Ok(_) => info!(
            "Labels successfully saved to temporary file '{}'",
            label_file_path.display()
        ),
        Err(e) => error!("Error saving labels: {}", e),
    }

    // 7. Demonstrate reading the temporary file back
    info!("\n--- Reading Labels Back from Temporary File ---");
    match Labels::try_from_file(&label_file_path) {
        // Pass &PathBuf ref
        Ok(reloaded_labels) => {
            info!("Successfully reloaded {} labels:", reloaded_labels.len());
            for label_entry in reloaded_labels.iter() {
                if let Some(label_text) = label_entry.label() {
                    info!(
                        "  {} -> {}",
                        format_ref_str(&label_entry.ref_()),
                        label_text
                    );
                }
            }
        }
        Err(e) => error!("Error reloading labels: {}", e),
    }

    info!("\n--- Example Finished ---");
    Ok(())
}

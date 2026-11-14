//! # BIP-329 labels with `bdk_wallet`
//!
//! `bdk_wallet` does not store labels. This example shows the application-owned approach: labels
//! live in a store the application controls (here, a JSONL file), and [BIP-329] is the
//! import/export format, handled by the `bip329` crate.
//!
//! It demonstrates how to:
//!
//! 1. Load a label store from disk, starting empty if the file does not exist yet.
//! 2. Add and update labels for wallet addresses and transactions, then save the store.
//! 3. Look up the labels for the wallet's transactions.
//! 4. Import labels from another BIP-329 file under an explicit merge policy, and choose which
//!    record types the application applies.
//!
//! Labels are keyed by record type *and* reference, for example `addr:<address>` or `tx:<txid>`.
//! `input` and `output` records share the same `txid:vout` reference format, so a bare reference
//! string can be ambiguous.
//!
//! The wallet is funded with test transactions.
//!
//! ```shell
//! cargo run --example bip329_labels
//! ```
//!
//! [BIP-329]: https://github.com/bitcoin/bips/blob/master/bip-0329.mediawiki

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use bdk_chain::BlockId;
use bdk_wallet::test_utils::{get_test_wpkh_and_change_desc, insert_checkpoint, insert_tx_anchor};
use bdk_wallet::{KeychainKind, Wallet};
use bip329::{AddressRecord, Label, Labels, OutputRecord, TransactionRecord};
use bitcoin::absolute::LockTime;
use bitcoin::transaction::Version;
use bitcoin::{Address, Amount, Network, OutPoint, Transaction, TxIn, TxOut, Txid};
use tempfile::tempdir;

const NETWORK: Network = Network::Regtest;

/// How to treat an incoming label whose key already exists in the store.
///
/// BIP-329 only defines the file format. Merge behavior is application policy.
#[derive(Clone, Copy, Debug)]
enum MergePolicy {
    /// Keep the label already in the store and ignore the incoming one.
    KeepExisting,
    /// Replace the stored label with the incoming one.
    Overwrite,
}

/// What a merge did, for reporting.
#[derive(Debug, Default, PartialEq)]
struct MergeReport {
    added: usize,
    replaced: usize,
    kept: usize,
    skipped: usize,
}

fn main() -> Result<()> {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();

    // Create the wallet and fund it. BDK owns the wallet state, and labels stay outside of it.
    let mut wallet = Wallet::create(desc, change_desc)
        .network(NETWORK)
        .create_wallet_no_persist()?;
    let [(addr_a, txid_a), (addr_b, txid_b)] = fund_wallet(&mut wallet)?;

    let dir = tempdir().context("failed to create temporary directory")?;
    let store_path = dir.path().join("wallet_labels.jsonl");
    let external_path = dir.path().join("labels_from_another_wallet.jsonl");

    // 1. Load the store. On a first run the file is missing and we start empty.
    println!("--- 1. Load the label store ---");
    let mut store = load_or_default(&store_path)?;
    println!(
        "Loaded {} labels from {}",
        store.len(),
        store_path.display()
    );
    assert!(store.is_empty());

    // 2. Add labels and save.
    println!("\n--- 2. Add labels and save ---");
    upsert(
        &mut store,
        address_label(&addr_a, "Primary receiving address"),
    );
    upsert(&mut store, tx_label(txid_a, "Mining reward"));
    store
        .export_to_file(&store_path)
        .map_err(|e| anyhow!("failed to save labels: {e}"))?;
    println!("Saved {} labels", store.len());

    // 3. Reload from disk. This time the file exists, so we read it back.
    println!("\n--- 3. Reload the store from disk ---");
    let mut store = load_or_default(&store_path)?;
    println!("Reloaded {} labels", store.len());
    assert_eq!(store.len(), 2);

    // 4. Look up labels for the wallet's transactions and addresses.
    println!("\n--- 4. Look up labels for wallet items ---");
    let lookup = build_lookup(&store);
    for wallet_tx in wallet.transactions() {
        println!(
            "{}",
            describe(&lookup, "Wallet tx", &tx_key(wallet_tx.tx_node.txid))
        );
    }
    println!("{}", describe(&lookup, "Address A", &addr_key(&addr_a)));
    println!("{}", describe(&lookup, "Address B", &addr_key(&addr_b)));
    assert_eq!(
        lookup.get(&tx_key(txid_a)).map(String::as_str),
        Some("Mining reward")
    );
    assert_eq!(lookup.get(&tx_key(txid_b)), None);

    // 5. Simulate a BIP-329 file exported by another wallet, then parse it. It conflicts on address
    //    A, adds address B and tx B, and has an `output` record.
    println!("\n--- 5. Read a BIP-329 file exported by another wallet ---");
    let incoming = Labels::new(vec![
        address_label(&addr_a, "Donations"),
        address_label(&addr_b, "Savings"),
        tx_label(txid_b, "Paid for hardware"),
        Label::Output(OutputRecord {
            ref_: OutPoint::new(txid_a, 0),
            label: Some("Do not mix".to_owned()),
            spendable: Some(false),
        }),
    ]);
    incoming
        .export_to_file(&external_path)
        .map_err(|e| anyhow!("failed to write the example import file: {e}"))?;
    let incoming = Labels::try_from_file(&external_path).context("failed to parse import file")?;
    println!(
        "Parsed {} records from {}",
        incoming.len(),
        external_path.display()
    );

    // 6. Same file, two policies. Parsing is the crate's job, the policy is bdk's.
    println!("\n--- 6. Merge under each policy ---");
    let cases = [
        (
            MergePolicy::KeepExisting,
            "Primary receiving address",
            MergeReport {
                added: 2,
                replaced: 0,
                kept: 1,
                skipped: 1,
            },
        ),
        (
            MergePolicy::Overwrite,
            "Donations",
            MergeReport {
                added: 2,
                replaced: 1,
                kept: 0,
                skipped: 1,
            },
        ),
    ];
    for (policy, expected_label, expected_report) in cases {
        let mut trial = store.clone();
        let report = merge(&mut trial, &incoming, policy);
        let after = build_lookup(&trial);
        println!("{policy:?}: {report:?}");
        println!("  {}", describe(&after, "Address A", &addr_key(&addr_a)));
        assert_eq!(report, expected_report);
        assert_eq!(
            after.get(&addr_key(&addr_a)).map(String::as_str),
            Some(expected_label)
        );
    }

    // 7. Apply one policy for real, make a final edit, and save.
    println!("\n--- 7. Apply Overwrite, update a label, save ---");
    let report = merge(&mut store, &incoming, MergePolicy::Overwrite);
    println!("Applied: {report:?}");
    upsert(&mut store, address_label(&addr_b, "Long-term savings"));
    store
        .export_to_file(&store_path)
        .map_err(|e| anyhow!("failed to save labels: {e}"))?;
    assert_eq!(store.len(), 4);

    println!("\nFinal store ({} labels), as BIP-329 JSONL:", store.len());
    store
        .export_to_writer(std::io::stdout())
        .map_err(|e| anyhow!("failed to print labels: {e}"))?;

    Ok(())
}

/// Fund the wallet with two received transactions, returning the address and txid of each.
fn fund_wallet(wallet: &mut Wallet) -> Result<[(Address, Txid); 2]> {
    let anchor_block = BlockId {
        height: 1,
        hash: "3bcc1c447c6b3886f43e416b5c21cf5c139dc4829a71dc78609bc8f6235611c5".parse()?,
    };
    let chain_tip = BlockId {
        height: 101,
        hash: "7f96292d115d19450e4bf7d4c4e15c9f3ad21e3a3cf616c498110b988963470b".parse()?,
    };

    let received = [
        receive(wallet, Amount::from_sat(50_000), anchor_block),
        receive(wallet, Amount::from_sat(20_000), anchor_block),
    ];
    insert_checkpoint(wallet, chain_tip);

    Ok(received)
}

/// Insert a transaction paying `value` to the wallet's next external address.
fn receive(wallet: &mut Wallet, value: Amount, block: BlockId) -> (Address, Txid) {
    let address = wallet.reveal_next_address(KeychainKind::External).address;
    let tx = Transaction {
        lock_time: LockTime::ZERO,
        version: Version::TWO,
        input: vec![TxIn::default()],
        output: vec![TxOut {
            script_pubkey: address.script_pubkey(),
            value,
        }],
    };
    let txid = tx.compute_txid();
    insert_tx_anchor(wallet, tx, block);
    (address, txid)
}

/// Store key for a record: record type plus reference.
///
/// Prefixing the type avoids collisions between record types that share a reference format
/// (`input` and `output` are both `txid:vout`).
fn key(label: &Label) -> String {
    match label {
        Label::Transaction(r) => format!("tx:{}", r.ref_),
        Label::Address(r) => format!("addr:{}", r.ref_.clone().assume_checked()),
        Label::PublicKey(r) => format!("pubkey:{}", r.ref_),
        Label::Input(r) => format!("input:{}", r.ref_),
        Label::Output(r) => format!("output:{}", r.ref_),
        Label::ExtendedPublicKey(r) => format!("xpub:{}", r.ref_),
    }
}

/// Key for a wallet address, matching [`key`].
fn addr_key(address: &Address) -> String {
    format!("addr:{address}")
}

/// Key for a wallet transaction, matching [`key`].
fn tx_key(txid: Txid) -> String {
    format!("tx:{txid}")
}

/// Build an address label. BIP-329 stores addresses as `NetworkUnchecked` because a label file
/// can come from any network.
fn address_label(address: &Address, text: &str) -> Label {
    Label::Address(AddressRecord {
        ref_: address.clone().into_unchecked(),
        label: Some(text.to_owned()),
    })
}

/// Build a transaction label.
fn tx_label(txid: Txid, text: &str) -> Label {
    Label::Transaction(TransactionRecord {
        ref_: txid,
        label: Some(text.to_owned()),
        origin: None,
    })
}

/// Application policy: which record types we apply on import.
///
/// This example keeps `tx` and `addr` records and skips the rest. Another application might keep
/// outputs too. `bip329` parses all types either way.
fn is_supported(label: &Label) -> bool {
    matches!(label, Label::Transaction(_) | Label::Address(_))
}

/// Load a label store, returning an empty one if the file does not exist yet.
fn load_or_default(path: &Path) -> Result<Labels> {
    match Labels::try_from_file(path) {
        Ok(labels) => Ok(labels),
        Err(bip329::error::ParseError::FileReadError(e)) if e.kind() == ErrorKind::NotFound => {
            Ok(Labels::default())
        }
        Err(e) => Err(anyhow!(
            "failed to load labels from {}: {e}",
            path.display()
        )),
    }
}

/// Insert a label, replacing any existing label with the same key.
fn upsert(labels: &mut Labels, new: Label) {
    let k = key(&new);
    let pos = labels.iter().position(|l| key(l) == k);
    match pos {
        Some(i) => labels[i] = new,
        None => labels.push(new),
    }
}

/// Merge `incoming` into `existing` under `policy`, applying only supported record types.
fn merge(existing: &mut Labels, incoming: &Labels, policy: MergePolicy) -> MergeReport {
    let mut report = MergeReport::default();
    for new in incoming.iter() {
        if !is_supported(new) {
            report.skipped += 1;
            continue;
        }
        let k = key(new);
        let pos = existing.iter().position(|l| key(l) == k);
        match (pos, policy) {
            (None, _) => {
                existing.push(new.clone());
                report.added += 1;
            }
            (Some(i), MergePolicy::Overwrite) => {
                existing[i] = new.clone();
                report.replaced += 1;
            }
            (Some(_), MergePolicy::KeepExisting) => report.kept += 1,
        }
    }
    report
}

/// Build a `key -> label text` lookup. Rebuild it after the store changes.
fn build_lookup(labels: &Labels) -> HashMap<String, String> {
    labels
        .iter()
        .filter_map(|l| l.label().map(|text| (key(l), text.to_owned())))
        .collect()
}

fn describe(lookup: &HashMap<String, String>, item: &str, k: &str) -> String {
    match lookup.get(k) {
        Some(text) => format!("{item} ({k}): {text}"),
        None => format!("{item} ({k}): [no label]"),
    }
}

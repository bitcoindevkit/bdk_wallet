//! # BDK Wallet + BIP-329 Labels Example
//!
//! This example demonstrates how to use [`bdk_wallet`] together with the
//! [`bip329`] crate to attach and persist human-readable labels to wallet
//! items (addresses, transactions, and outputs) in the standard BIP-329 JSONL
//! format.
//!
//! ## What this example covers
//!
//! 1. Creating a BDK wallet from a generated BIP-39 mnemonic.
//! 2. Revealing receive addresses and labelling them.
//! 3. Building `TransactionRecord`s for wallet transactions.
//! 4. Labelling UTXOs via `OutputRecord` with `spendable` coin-control hints.
//! 5. Exporting all labels to a BIP-329 JSONL file.
//! 6. Reloading those labels and doing efficient lookups with `Labels::into_string_map`.
//! 7. Updating a label in-place (`retain` + `push`) and re-exporting to stdout so you can see the
//!    raw JSONL format.
//!
//! ## Running
//!
//! ```shell
//! cargo run --example bip329_labels --features keys-bip39
//! ```

use anyhow::{anyhow, Context, Result};
use bdk_wallet::{
    keys::{
        bip39::{Language, Mnemonic, WordCount},
        DerivableKey, ExtendedKey, GeneratableKey, GeneratedKey,
    },
    miniscript, AddressInfo, KeychainKind, Wallet,
};
use bip329::{AddressRecord, Label, Labels, OutputRecord, TransactionRecord};
use bitcoin::{address::NetworkUnchecked, Address, Network, OutPoint, Txid};
use std::{io::ErrorKind, path::Path, str::FromStr};
use tempfile::tempdir;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Derive BIP-84 (native SegWit) receive + change descriptors from a mnemonic.
fn descriptors_from_mnemonic(mnemonic: &Mnemonic, network: Network) -> Result<(String, String)> {
    let xkey: ExtendedKey = mnemonic
        .clone()
        .into_extended_key()
        .context("mnemonic → xkey")?;
    let xprv = xkey
        .into_xprv(network.into())
        .ok_or_else(|| anyhow!("could not derive xprv for {network}"))?;

    // BIP-84: m/84h/coin_typeh/0h/{0,1}/*
    let coin = match network {
        Network::Bitcoin => 0,
        _ => 1,
    };
    Ok((
        format!("wpkh({xprv}/84h/{coin}h/0h/0/*)"),
        format!("wpkh({xprv}/84h/{coin}h/0h/1/*)"),
    ))
}

/// Load a `Labels` collection from `path`, returning an empty one if the file
/// does not yet exist.
fn load_or_default(path: &Path) -> Result<Labels> {
    match Labels::try_from_file(path) {
        Ok(l) => Ok(l),
        Err(bip329::error::ParseError::FileReadError(e)) if e.kind() == ErrorKind::NotFound => {
            Ok(Labels::default())
        }
        Err(e) => Err(anyhow!(
            "failed to load labels from {}: {e}",
            path.display()
        )),
    }
}

/// Return a BIP-84 derivation path string for a BDK `AddressInfo`.
///
/// BIP-329 does not have a dedicated `keypath` field on `AddressRecord` in the
/// current crate version, but the derivation path is useful context to print
/// alongside labels so that other wallets can verify or re-derive the address.
fn keypath_for(info: &AddressInfo, network: Network) -> String {
    let coin = match network {
        Network::Bitcoin => 0,
        _ => 1,
    };
    let change = match info.keychain {
        KeychainKind::External => 0,
        KeychainKind::Internal => 1,
    };
    format!("m/84h/{coin}h/0h/{change}/{}", info.index)
}

// ── main ─────────────────────────────────────────────────────────────────────

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    // ── 1. Create a BDK wallet from a freshly generated mnemonic ─────────────

    let network = Network::Regtest;

    let mnemonic: GeneratedKey<_, miniscript::Segwitv0> =
        Mnemonic::generate((WordCount::Words12, Language::English))
            .map_err(|_| anyhow!("mnemonic generation failed"))?;
    let mnemonic = Mnemonic::parse_in(Language::English, mnemonic.to_string())?;

    println!("Mnemonic: {mnemonic}");

    let (ext_desc, int_desc) = descriptors_from_mnemonic(&mnemonic, network)?;

    let mut wallet = Wallet::create(ext_desc, int_desc)
        .network(network)
        .create_wallet_no_persist()
        .context("wallet creation failed")?;

    println!("\n── Wallet ready ({network}) ─────────────────────────────────\n");

    // ── 2. Reveal addresses and build Address labels ──────────────────────────
    //
    // `reveal_next_address` increments the derivation index. Call
    // `wallet.persist(&mut conn)` after each reveal so the new index is saved
    // to disk — skipping this step risks handing out the same address twice.

    let savings = wallet.reveal_next_address(KeychainKind::External);
    let exchange = wallet.reveal_next_address(KeychainKind::External);
    let change = wallet.reveal_next_address(KeychainKind::Internal);

    // BIP-329 `AddressRecord` stores the address and an optional label string.
    // The address must be in `NetworkUnchecked` form as bip329 accepts labels
    // for any network.
    let to_unchecked = |addr: &Address| -> Address<NetworkUnchecked> {
        Address::from_str(&addr.to_string())
            .expect("address was just derived")
            .into_unchecked()
    };

    // Build the label vec — we use `Label::Address(AddressRecord { .. })`.
    // Note: in addition to the label string it is good practice to log the
    // BIP-84 derivation path alongside the record for cross-wallet portability.
    let mut labels: Vec<Label> = vec![
        Label::Address(AddressRecord {
            ref_: to_unchecked(&savings.address),
            label: Some("Long-term savings".to_owned()),
        }),
        Label::Address(AddressRecord {
            ref_: to_unchecked(&exchange.address),
            label: Some("Exchange deposit address".to_owned()),
        }),
        Label::Address(AddressRecord {
            ref_: to_unchecked(&change.address),
            label: Some("Internal change address".to_owned()),
        }),
    ];

    println!("Address labels:");
    for addr in [&savings, &exchange, &change] {
        let kind = match addr.keychain {
            KeychainKind::External => "ext",
            KeychainKind::Internal => "int",
        };
        println!(
            "  [{}] {} → keypath: {}",
            kind,
            addr.address,
            keypath_for(addr, network),
        );
    }

    // ── 3. Transaction labels with wallet-derived metadata ───────────────────
    //
    // After syncing the wallet via a chain source (Electrum, Esplora, etc.),
    // iterate `wallet.transactions()` to visit every canonical transaction.
    // For each `WalletTx` you can enrich the `TransactionRecord` with:
    //
    //   * `wallet.calculate_fee(&wtx.tx_node.tx)` → fee in satoshis
    //   * `wallet.sent_and_received(&wtx.tx_node.tx)` → net flow as `Amount`
    //
    // Two well-known txids stand in for synced transactions here so the
    // example runs without a live chain connection.

    let incoming_txid =
        Txid::from_str("f4184fc596403b9d638783cf57adfe4c75c605f6356fbc91338530e9831e9e16")?;
    let outgoing_txid =
        Txid::from_str("a1075db55d416d3ca199f55b6084e2115b9345e16c5cf302fc80e9d5fbf5d48d")?;

    labels.push(Label::Transaction(TransactionRecord {
        ref_: incoming_txid,
        label: Some("Received from mining pool".to_owned()),
        // `origin` records which descriptor output was involved, e.g.
        // `"wpkh([d34db33f/84'/1'/0'])"`.  Leave `None` when unknown.
        origin: None,
    }));

    labels.push(Label::Transaction(TransactionRecord {
        ref_: outgoing_txid,
        label: Some("Paid exchange deposit — 0.5 BTC".to_owned()),
        origin: None,
    }));

    println!("\nTransaction labels:");
    println!("  {incoming_txid}  →  Received from mining pool");
    println!("  {outgoing_txid}  →  Paid exchange deposit — 0.5 BTC");

    // ── 4. UTXO labels with coin-control hints ────────────────────────────────
    //
    // `OutputRecord` labels a specific UTXO by its `OutPoint` (txid:vout).
    // The `spendable` flag signals coin-control UIs whether to include this
    // output in automatic coin selection — set it to `false` to quarantine a
    // coin (e.g. a privacy-sensitive or dust output).
    //
    // Iterate `wallet.list_unspent()` to get `LocalOutput` values after a
    // sync, then use `local_output.outpoint` as the `ref_` for each record.

    let utxo_locked = OutPoint::new(incoming_txid, 0);
    let utxo_change = OutPoint::new(incoming_txid, 1);

    labels.push(Label::Output(OutputRecord {
        ref_: utxo_locked,
        label: Some("Mining reward — do not mix".to_owned()),
        // Mark as non-spendable so coin-selection skips it by default.
        spendable: false,
    }));

    labels.push(Label::Output(OutputRecord {
        ref_: utxo_change,
        label: Some("Change from savings top-up".to_owned()),
        spendable: true,
    }));

    println!("\nUTXO labels:");
    println!("  {utxo_locked}  →  Mining reward (spendable: false)");
    println!("  {utxo_change}  →  Change coin   (spendable: true)");

    // ── 5. Export to a BIP-329 JSONL file ────────────────────────────────────

    let dir = tempdir().context("tempdir")?;
    let label_file = dir.path().join("wallet_labels.jsonl");

    let bip329_labels = Labels::new(labels);
    bip329_labels
        .export_to_file(&label_file)
        .map_err(|e| anyhow!("export failed: {e}"))?;

    println!(
        "\n── Exported {} labels to {} ───────",
        bip329_labels.len(),
        label_file.display(),
    );

    // ── 6. Reload and query via string map ────────────────────────────────────
    //
    // `Labels::into_string_map` returns `HashMap<String, Label>` keyed by
    // the ref string.  This is the most convenient way to look up labels for
    // BDK items because BDK addresses and txids both serialise to plain
    // strings — no manual HashMap building needed.

    let reloaded = load_or_default(&label_file)?;
    println!(
        "\n── Reloaded {} labels from file ────────────",
        reloaded.len()
    );

    let label_map = reloaded.clone().into_string_map();

    // Look up the savings address label using BDK's string representation.
    let savings_key = savings.address.to_string();
    match label_map.get(&savings_key) {
        Some(lbl) => println!(
            "  Savings address:    {:?}",
            lbl.label().unwrap_or("<none>")
        ),
        None => println!("  Savings address: [no label]"),
    }

    // Look up the incoming transaction label.
    let incoming_key = incoming_txid.to_string();
    match label_map.get(&incoming_key) {
        Some(lbl) => println!(
            "  Incoming tx label:  {:?}",
            lbl.label().unwrap_or("<none>")
        ),
        None => println!("  Incoming tx: [no label]"),
    }

    // Look up a UTXO label — OutPoint key is `txid:vout`.
    let utxo_key = format!("{}:{}", utxo_locked.txid, utxo_locked.vout);
    match label_map.get(&utxo_key) {
        Some(lbl) => println!(
            "  Locked UTXO label:  {:?}",
            lbl.label().unwrap_or("<none>")
        ),
        None => println!("  Locked UTXO: [no label]"),
    }

    // ── 7. Update a label and re-export ──────────────────────────────────────
    //
    // `Labels` derefs to `Vec<Label>`, so standard Vec operations apply.
    // The idiomatic update pattern is `retain` (remove old) + `push` (add new).

    let mut updated = reloaded;

    let savings_unchecked = to_unchecked(&savings.address);
    // Remove the existing savings label …
    updated.retain(|l| !matches!(l, Label::Address(r) if r.ref_ == savings_unchecked));
    // … and push the revised one.
    updated.push(Label::Address(AddressRecord {
        ref_: savings_unchecked,
        label: Some("Long-term savings (cold storage — hardware wallet)".to_owned()),
    }));

    // Re-export to stdout so you can inspect the raw BIP-329 JSONL format.
    println!("\n── Updated JSONL (stdout) ───────────────────────────────────");
    updated
        .export_to_writer(std::io::stdout())
        .map_err(|e| anyhow!("export to writer failed: {e}"))?;

    println!("\n── Done ─────────────────────────────────────────────────────");
    Ok(())
}

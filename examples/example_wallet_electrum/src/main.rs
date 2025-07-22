use bdk_wallet::bitcoin::Txid;
use bdk_wallet::file_store::Store;
use bdk_wallet::Wallet;
use std::io::Write;

use bdk_electrum::electrum_client;
use bdk_electrum::BdkElectrumClient;
use bdk_wallet::bitcoin::Amount;
use bdk_wallet::bitcoin::Network;
use bdk_wallet::chain::collections::HashSet;
use bdk_wallet::{KeychainKind, SignOptions};

const DB_MAGIC: &str = "bdk_wallet_electrum_example";
const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const STOP_GAP: usize = 50;
const BATCH_SIZE: usize = 5;

const NETWORK: Network = Network::Testnet;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ELECTRUM_URL: &str = "ssl://electrum.blockstream.info:60002";

fn main() -> Result<(), anyhow::Error> {
    let db_path = "bdk-electrum-example.db";

    let (mut db, _) = Store::<bdk_wallet::ChangeSet>::load_or_create(DB_MAGIC.as_bytes(), db_path)?;

    let wallet_opt = Wallet::load()
        .descriptor(KeychainKind::External, Some(EXTERNAL_DESC))
        .descriptor(KeychainKind::Internal, Some(INTERNAL_DESC))
        .extract_keys()
        .check_network(NETWORK)
        .load_wallet(&mut db)?;
    let mut wallet = match wallet_opt {
        Some(wallet) => wallet,
        None => Wallet::create(EXTERNAL_DESC, INTERNAL_DESC)
            .network(NETWORK)
            .create_wallet(&mut db)?,
    };

    let address = wallet.next_unused_address(KeychainKind::External);
    wallet.persist(&mut db)?;
    println!("Generated Address: {address}");

    let balance = wallet.balance();
    println!("Wallet balance before syncing: {}", balance.total());

    println!("=== Performing Full Sync ===");
    let client = BdkElectrumClient::new(electrum_client::Client::new(ELECTRUM_URL)?);

    // Populate the electrum client's transaction cache so it doesn't redownload transaction we
    // already have.
    client.populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));

    let request = wallet.start_full_scan().inspect({
        let mut stdout = std::io::stdout();
        let mut once = HashSet::<KeychainKind>::new();
        move |k, spk_i, _| {
            if once.insert(k) {
                print!("\nScanning keychain [{k:?}]");
            }
            print!(" {spk_i:<3}");
            stdout.flush().expect("must flush");
        }
    });

    let update = client.full_scan(request, STOP_GAP, BATCH_SIZE, false)?;

    println!();

    wallet.apply_update(update)?;
    wallet.persist(&mut db)?;

    let balance = wallet.balance();
    println!("Wallet balance after full sync: {}", balance.total());
    println!(
        "Wallet has {} transactions and {} utxos after full sync",
        wallet.transactions().count(),
        wallet.list_unspent().count()
    );

    if balance.total() < SEND_AMOUNT {
        println!("Please send at least {SEND_AMOUNT} to the receiving address");
        std::process::exit(0);
    }

    let mut tx_builder = wallet.build_tx();
    tx_builder.add_recipient(address.script_pubkey(), SEND_AMOUNT);

    let mut psbt = tx_builder.finish()?;
    let finalized = wallet.sign(&mut psbt, SignOptions::default())?;
    assert!(finalized);

    let tx = psbt.extract_tx()?;
    client.transaction_broadcast(&tx)?;
    println!("Tx broadcasted! Txid: {}", tx.compute_txid());

    let unconfirmed_txids: HashSet<Txid> = wallet
        .transactions()
        .filter(|tx| tx.chain_position.is_unconfirmed())
        .map(|tx| tx.tx_node.txid)
        .collect();

    client.populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));

    println!("\n=== Performing Partial Sync ===\n");
    print!("SCANNING: ");
    let mut last_printed = 0;
    let sync_request = wallet
        .start_sync_with_revealed_spks()
        .inspect(move |_, sync_progress| {
            let progress_percent =
                (100 * sync_progress.consumed()) as f32 / sync_progress.total() as f32;
            let progress_percent = progress_percent.round() as u32;
            if progress_percent.is_multiple_of(5) && progress_percent > last_printed {
                print!("{progress_percent}% ");
                std::io::stdout().flush().expect("must flush");
                last_printed = progress_percent;
            }
        });
    client.populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));
    let sync_update = client.sync(sync_request, BATCH_SIZE, false)?;
    println!();

    let mut evicted_txs = Vec::new();
    for txid in unconfirmed_txids {
        let tx_node = wallet
            .tx_graph()
            .full_txs()
            .find(|full_tx| full_tx.txid == txid);
        let wallet_tx = wallet.get_tx(txid);

        let is_evicted = match wallet_tx {
            Some(wallet_tx) => {
                !wallet_tx.chain_position.is_unconfirmed()
                    && !wallet_tx.chain_position.is_confirmed()
            }
            None => true,
        };

        if is_evicted {
            if let Some(full_tx) = tx_node {
                evicted_txs.push((full_tx.txid, full_tx.last_seen.unwrap_or(0)));
            } else {
                evicted_txs.push((txid, 0));
            }
        }
    }

    if !evicted_txs.is_empty() {
        wallet.apply_evicted_txs(evicted_txs.clone());
        println!("Applied {} evicted transactions", evicted_txs.len());
    }

    wallet.apply_update(sync_update)?;
    wallet.persist(&mut db)?;

    let balance_after_sync = wallet.balance();
    println!("Wallet balance after sync: {}", balance_after_sync.total());
    println!(
        "Wallet has {} transactions and {} utxos after partial sync",
        wallet.transactions().count(),
        wallet.list_unspent().count()
    );

    Ok(())
}

use bdk_esplora::{esplora_client, EsploraExt};
use bdk_wallet::{
    bitcoin::{Amount, Network, Txid},
    file_store::Store,
    KeychainKind, SignOptions, Wallet,
};
use std::{
    collections::{BTreeSet, HashSet},
    io::Write,
};

const DB_MAGIC: &str = "bdk_wallet_esplora_example";
const DB_PATH: &str = "bdk-example-esplora-blocking.db";
const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

const NETWORK: Network = Network::Signet;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ESPLORA_URL: &str = "http://signet.bitcoindevkit.net";

fn main() -> Result<(), anyhow::Error> {
    let (mut db, _) = Store::<bdk_wallet::ChangeSet>::load_or_create(DB_MAGIC.as_bytes(), DB_PATH)?;

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
    println!(
        "Next unused address: ({}) {}",
        address.index, address.address
    );

    let balance = wallet.balance();
    println!("Wallet balance before syncing: {}", balance.total());

    print!("Syncing...");
    let client = esplora_client::Builder::new(ESPLORA_URL).build_blocking();

    let request = wallet.start_full_scan().inspect({
        let mut stdout = std::io::stdout();
        let mut once = BTreeSet::<KeychainKind>::new();
        move |keychain, spk_i, _| {
            if once.insert(keychain) {
                print!("\nScanning keychain [{keychain:?}] ");
            }
            print!(" {spk_i:<3}");
            stdout.flush().expect("must flush")
        }
    });

    let update = client.full_scan(request, STOP_GAP, PARALLEL_REQUESTS)?;

    wallet.apply_update(update)?;
    wallet.persist(&mut db)?;
    println!();

    let balance = wallet.balance();
    println!("Wallet balance after syncing: {}", balance.total());

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
    client.broadcast(&tx)?;
    println!("Tx broadcasted! Txid: {}", tx.compute_txid());

    let unconfirmed_txids: HashSet<Txid> = wallet
        .transactions()
        .filter(|tx| tx.chain_position.is_unconfirmed())
        .map(|tx| tx.tx_node.txid)
        .collect();

    println!("\n=== Performing Partial Sync ===\n");
    print!("SCANNING: ");
    let mut printed = 0;
    let sync_request = wallet
        .start_sync_with_revealed_spks()
        .inspect(move |_, sync_progress| {
            let progress_percent =
                (100 * sync_progress.consumed()) as f32 / sync_progress.total() as f32;
            let progress_percent = progress_percent.round() as u32;
            if progress_percent.is_multiple_of(5) && progress_percent > printed {
                print!("{progress_percent}% ");
                std::io::stdout().flush().expect("must flush");
                printed = progress_percent;
            }
        });
    let sync_update = client.sync(sync_request, PARALLEL_REQUESTS)?;
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
        let evicted_count = evicted_txs.len();
        wallet.apply_evicted_txs(evicted_txs);
        println!("Applied {evicted_count} evicted transactions");
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

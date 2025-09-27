use bdk_electrum::electrum_client;
use bdk_electrum::BdkElectrumClient;
use bdk_wallet::bitcoin::Amount;
use bdk_wallet::bitcoin::FeeRate;
use bdk_wallet::bitcoin::Network;
use bdk_wallet::chain::collections::HashSet;
use bdk_wallet::psbt::PsbtUtils;
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::Wallet;
use bdk_wallet::{KeychainKind, SignOptions};
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const STOP_GAP: usize = 50;
const BATCH_SIZE: usize = 5;

const DB_PATH: &str = "bdk-example-electrum.sqlite";
const NETWORK: Network = Network::Testnet4;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ELECTRUM_URL: &str = "ssl://mempool.space:40002";

fn main() -> Result<(), anyhow::Error> {
    // let mut db = Connection::open(DB_PATH)?;
    // let wallet_opt = Wallet::load()
    //     .descriptor(KeychainKind::External, Some(EXTERNAL_DESC))
    //     .descriptor(KeychainKind::Internal, Some(INTERNAL_DESC))
    //     .extract_keys()
    //     .check_network(NETWORK)
    //     .load_wallet(&mut db)?;
    // let mut wallet = match wallet_opt {
    //     Some(wallet) => wallet,
    //     None => Wallet::create(EXTERNAL_DESC, INTERNAL_DESC)
    //         .network(NETWORK)
    //         .create_wallet(&mut db)?,
    // };

    // let address = wallet.next_unused_address(KeychainKind::External);
    // wallet.persist(&mut db)?;
    // println!("Generated Address: {address}");

    // let balance = wallet.balance();
    // println!("Wallet balance before syncing: {}", balance.total());

    // println!("Performing Full Sync...");
    // let client = BdkElectrumClient::new(electrum_client::Client::new(ELECTRUM_URL)?);

    // // Populate the electrum client's transaction cache so it doesn't redownload transaction we
    // // already have.
    // client.populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));

    // let request = wallet.start_full_scan().inspect({
    //     let mut stdout = std::io::stdout();
    //     let mut once = HashSet::<KeychainKind>::new();
    //     move |k, spk_i, _| {
    //         if once.insert(k) {
    //             print!("\nScanning keychain [{k:?}]");
    //         }
    //         if spk_i.is_multiple_of(5) {
    //             print!(" {spk_i:<3}");
    //         }
    //         stdout.flush().expect("must flush");
    //     }
    // });

    // let update = client.full_scan(request, STOP_GAP, BATCH_SIZE, false)?;

    // println!();

    // wallet.apply_update(update)?;
    // wallet.persist(&mut db)?;

    // let balance = wallet.balance();
    // println!("Wallet balance after full sync: {}", balance.total());
    // println!(
    //     "Wallet has {} transactions and {} utxos after full sync",
    //     wallet.transactions().count(),
    //     wallet.list_unspent().count()
    // );

    // if balance.total() < SEND_AMOUNT {
    //     println!("Please send at least {SEND_AMOUNT} to the receiving address");
    //     std::process::exit(0);
    // }

    // let target_fee_rate = FeeRate::from_sat_per_vb(1).unwrap();
    // let mut tx_builder = wallet.build_tx();
    // tx_builder.add_recipient(address.script_pubkey(), SEND_AMOUNT);
    // tx_builder.fee_rate(target_fee_rate);

    // let mut psbt = tx_builder.finish()?;
    // let finalized = wallet.sign(&mut psbt, SignOptions::default())?;
    // assert!(finalized);
    // let original_fee = psbt.fee_amount().unwrap();
    // let tx_feerate = psbt.fee_rate().unwrap();
    // let tx = psbt.extract_tx()?;
    // client.transaction_broadcast(&tx)?;
    // let txid = tx.compute_txid();
    // println!("Tx broadcasted! Txid: https://mempool.space/testnet4/tx/{txid}");

    // println!("Partial Sync...");
    // print!("SCANNING: ");
    // let mut last_printed = 0;
    // let sync_request = wallet
    //     .start_sync_with_revealed_spks()
    //     .inspect(move |_, sync_progress| {
    //         let progress_percent =
    //             (100 * sync_progress.consumed()) as f32 / sync_progress.total() as f32;
    //         let progress_percent = progress_percent.round() as u32;
    //         if progress_percent.is_multiple_of(5) && progress_percent > last_printed {
    //             print!("{progress_percent}% ");
    //             std::io::stdout().flush().expect("must flush");
    //             last_printed = progress_percent;
    //         }
    //     });
    // client.populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));
    // let sync_update = client.sync(sync_request, BATCH_SIZE, false)?;
    // println!();
    // wallet.apply_update(sync_update)?;
    // wallet.persist(&mut db)?;

    // // bump fee rate for tx by at least 1 sat per vbyte
    // let feerate = FeeRate::from_sat_per_vb(tx_feerate.to_sat_per_vb_ceil() + 1).unwrap();
    // let mut builder = wallet.build_fee_bump(txid).expect("failed to bump tx");
    // builder.fee_rate(feerate);
    // let mut bumped_psbt = builder.finish().unwrap();
    // let finalize_btx = wallet.sign(&mut bumped_psbt, SignOptions::default())?;
    // assert!(finalize_btx);
    // let new_fee = bumped_psbt.fee_amount().unwrap();
    // let bumped_tx = bumped_psbt.extract_tx()?;
    // assert_eq!(
    //     bumped_tx
    //         .output
    //         .iter()
    //         .find(|txout| txout.script_pubkey == address.script_pubkey())
    //         .unwrap()
    //         .value,
    //     SEND_AMOUNT,
    //     "Recipient output should remain unchanged"
    // );
    // assert!(
    //     new_fee > original_fee,
    //     "New fee ({new_fee}) should be higher than original ({original_fee})"
    // );

    // // wait for first transaction to make it into the mempool and be indexed on mempool.space
    // sleep(Duration::from_secs(10));
    // client.transaction_broadcast(&bumped_tx)?;
    // println!(
    //     "Broadcasted bumped tx. Txid: https://mempool.space/testnet4/tx/{}",
    //     bumped_tx.compute_txid()
    // );

    // println!("Syncing after bumped tx broadcast...");
    // let sync_request = wallet.start_sync_with_revealed_spks().inspect(|_, _| {});
    // let sync_update = client.sync(sync_request, BATCH_SIZE, false)?;

    // let mut evicted_txs = Vec::new();
    // for (txid, last_seen) in &sync_update.tx_update.evicted_ats {
    //     evicted_txs.push((*txid, *last_seen));
    // }

    // wallet.apply_update(sync_update)?;
    // if !evicted_txs.is_empty() {
    //     println!("Applied {} evicted transactions", evicted_txs.len());
    // }
    // wallet.persist(&mut db)?;

    // let balance_after_sync = wallet.balance();
    // println!("Wallet balance after sync: {}", balance_after_sync.total());
    // println!(
    //     "Wallet has {} transactions and {} utxos after partial sync",
    //     wallet.transactions().count(),
    //     wallet.list_unspent().count()
    // );

    Ok(())
}

#![allow(unused)]
use bdk_esplora::{esplora_client, EsploraAsyncExt};
use bdk_wallet::{
    bitcoin::{Amount, FeeRate, Network},
    psbt::PsbtUtils,
    rusqlite::Connection,
    KeychainKind, SignOptions, Wallet,
};
use std::{collections::BTreeSet, io::Write};
use tokio::time::{sleep, Duration};

const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

const DB_PATH: &str = "bdk-example-esplora-async.sqlite";
const NETWORK: Network = Network::Testnet4;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ESPLORA_URL: &str = "https://mempool.space/testnet4/api";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
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
    // println!("Next unused address: ({}) {address}", address.index);

    // let balance = wallet.balance();
    // println!("Wallet balance before syncing: {}", balance.total());

    // println!("Full Sync...");
    // let client = esplora_client::Builder::new(ESPLORA_URL).build_async()?;

    // let request = wallet.start_full_scan().inspect({
    //     let mut stdout = std::io::stdout();
    //     let mut once = BTreeSet::<KeychainKind>::new();
    //     move |keychain, spk_i, _| {
    //         if once.insert(keychain) {
    //             print!("\nScanning keychain [{keychain:?}]");
    //         }
    //         print!(" {spk_i:<3}");
    //         stdout.flush().expect("must flush")
    //     }
    // });

    // let update = client
    //     .full_scan(request, STOP_GAP, PARALLEL_REQUESTS)
    //     .await?;

    // wallet.apply_update(update)?;
    // wallet.persist(&mut db)?;
    // println!();

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
    // client.broadcast(&tx).await?;
    // let txid = tx.compute_txid();
    // println!("Tx broadcasted! Txid: https://mempool.space/testnet4/tx/{txid}");

    // println!("Partial Sync...");
    // print!("SCANNING: ");
    // let mut printed: u32 = 0;
    // let sync_request = wallet
    //     .start_sync_with_revealed_spks()
    //     .inspect(move |_, sync_progress| {
    //         let progress_percent =
    //             (100 * sync_progress.consumed()) as f32 / sync_progress.total() as f32;
    //         let progress_percent = progress_percent.round() as u32;
    //         if progress_percent % 5 == 0 && progress_percent > printed {
    //             print!("{progress_percent}% ");
    //             std::io::stdout().flush().expect("must flush");
    //             printed = progress_percent;
    //         }
    //     });
    // let sync_update = client.sync(sync_request, PARALLEL_REQUESTS).await?;
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
    //     "Outputs should be the same"
    // );
    // assert!(
    //     new_fee > original_fee,
    //     "New fee ({new_fee}) should be higher than original ({original_fee})",
    // );

    // // wait for first transaction to make it into the mempool and be indexed on mempool.space
    // sleep(Duration::from_secs(10)).await;
    // client.broadcast(&bumped_tx).await?;
    // println!(
    //     "Broadcasted bumped tx. Txid: https://mempool.space/testnet4/tx/{}",
    //     bumped_tx.compute_txid()
    // );

    // println!("syncing after broadcasting bumped tx...");
    // print!("SCANNING: ");
    // let sync_request = wallet
    //     .start_sync_with_revealed_spks()
    //     .inspect(move |_, sync_progress| {
    //         let progress_percent =
    //             (100 * sync_progress.consumed()) as f32 / sync_progress.total() as f32;
    //         let progress_percent = progress_percent.round() as u32;
    //         if progress_percent % 10 == 0 && progress_percent > printed {
    //             print!("{progress_percent}% ");
    //             std::io::stdout().flush().expect("must flush");
    //             printed = progress_percent;
    //         }
    //     });
    // let sync_update = client.sync(sync_request, PARALLEL_REQUESTS).await?;
    // println!();

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

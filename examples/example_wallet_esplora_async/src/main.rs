#![allow(unused)]
use std::{collections::BTreeSet, io::Write};

use anyhow::Ok;
use bdk_esplora::{esplora_client, EsploraAsyncExt};
use bdk_wallet::{
    bitcoin::{Amount, Network},
    chain::{DescriptorExt, DescriptorId},
    rusqlite::Connection,
    ChangeSet, CreateParams, Keychain, Keyring, SignOptions, Wallet,
};

const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

// const DB_PATH: &str = "bdk-example-esplora-async.sqlite";
const DB_PATH: &str = ".bdk_example_wallet_esplora_async.sqlite";
const NETWORK: Network = Network::Signet;
// const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
// const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ESPLORA_URL: &str = "http://signet.bitcoindevkit.net";

const MULTIPATH_DESCRIPTOR: &str = "wpkh([e273fe42/84'/1'/0']tpubDCmr3Luq75npLaYmRqqW1rLfSbfpnBXwLwAmUbR333fp95wjCHar3zoc9zSWovZFwrWr53mm3NTVqt6d1Pt6G26uf4etQjc3Pr5Hxe9QEQ2/<0;1>/*)";
const PK_DESCRIPTOR: &str = "tr(b511bd5771e47ee27558b1765e87b541668304ec567721c7b880edc0a010da55)";
// Desc ID of pk descriptor: "ef8a67b77b83797a1ad56504cc79e8c6990408265f1afdce72990b8a5baf7d3b"

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // let mut conn = Connection::open(DB_PATH)?;
    let mut conn = Connection::open_in_memory()?;

    // Setup: Initialize Keyring from a list of descriptors
    let mut keyring = Keyring::new(NETWORK);
    let _ = keyring.add_descriptors([MULTIPATH_DESCRIPTOR, PK_DESCRIPTOR])?;

    // Test 1: Create wallet with keyring and params
    let mut wallet = Wallet::with_keyring(keyring.clone())
        .network(NETWORK)
        .create_wallet(&mut conn)?;

    assert_eq!(wallet.keychains().count(), 3);
    let desc_id: DescriptorId =
        "ef8a67b77b83797a1ad56504cc79e8c6990408265f1afdce72990b8a5baf7d3b".parse()?;
    let (keychain, index, addr) = wallet.new_address(desc_id).expect("should reveal address");
    println!("New address: {:?} {}", (keychain, index), addr);

    // Test 2: Persist the keyring first and then load wallet from changeset
    // let changeset = keyring.initial_changeset();
    // let tx = conn.transaction()?;
    // ChangeSet::init_sqlite_tables(&tx)?;
    // changeset.persist_to_sqlite(&tx)?;
    // tx.commit()?;

    // let wallet = Wallet::load()
    //     .load_wallet(&mut conn)?
    //     .expect("should have persisted wallet");

    // assert_eq!(wallet.keychains().count(), 3);

    // More example code

    // let wallet_opt = Wallet::load()
    //     .descriptor(0, Some(EXTERNAL_DESC))
    //     .extract_keys()
    //     .check_network(NETWORK)
    //     .load_wallet(&mut conn)?;
    // let mut wallet = match wallet_opt {
    //     Some(wallet) => wallet,
    //     None => Wallet::create(EXTERNAL_DESC, INTERNAL_DESC)
    //         .network(NETWORK)
    //         .create_wallet(&mut conn)?,
    // };

    // let address = wallet.next_unused_address(Keychain::ZERO);
    // wallet.persist(&mut conn)?;
    // println!("Next unused address: ({}) {}", address.index, address);

    // let balance = wallet.balance();
    // println!("Wallet balance before syncing: {}", balance.total());

    // print!("Syncing...");
    // let client = esplora_client::Builder::new(ESPLORA_URL).build_async()?;

    // let request = wallet.start_full_scan().inspect({
    //     let mut stdout = std::io::stdout();
    //     let mut once = BTreeSet::<Keychain>::new();
    //     move |keychain, spk_i, _| {
    //         if once.insert(keychain) {
    //             print!("\nScanning keychain [{:?}]", keychain);
    //         }
    //         print!(" {:<3}", spk_i);
    //         stdout.flush().expect("must flush")
    //     }
    // });

    // let update = client
    //     .full_scan(request, STOP_GAP, PARALLEL_REQUESTS)
    //     .await?;

    // wallet.apply_update(update)?;
    // wallet.persist(&mut conn)?;
    // println!();

    // let balance = wallet.balance();
    // println!("Wallet balance after syncing: {}", balance.total());

    // if balance.total() < SEND_AMOUNT {
    //     println!(
    //         "Please send at least {} to the receiving address",
    //         SEND_AMOUNT
    //     );
    //     std::process::exit(0);
    // }

    // let mut tx_builder = wallet.build_tx();
    // tx_builder.add_recipient(address.script_pubkey(), SEND_AMOUNT);

    // let mut psbt = tx_builder.finish()?;
    // let finalized = wallet.sign(&mut psbt, SignOptions::default())?;
    // assert!(finalized);

    // let tx = psbt.extract_tx()?;
    // client.broadcast(&tx).await?;
    // println!("Tx broadcasted! Txid: {}", tx.compute_txid());

    Ok(())
}

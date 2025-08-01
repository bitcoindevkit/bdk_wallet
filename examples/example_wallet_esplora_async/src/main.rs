use std::{collections::BTreeSet, io::Write};

use anyhow::Ok;
use bdk_esplora::{esplora_client, EsploraAsyncExt};
use bdk_wallet::{
    bitcoin::{Amount, Network},
    miniscript::Descriptor,
    rusqlite::Connection,
    KeychainKind, SignOptions, Wallet,
};

const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

const DB_PATH: &str = "bdk-example-esplora-async.sqlite";
const NETWORK: Network = Network::Signet;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ESPLORA_URL: &str = "http://signet.bitcoindevkit.net";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut conn = Connection::open(DB_PATH)?;

    let wallet_opt = Wallet::load()
        .descriptor(KeychainKind::External, Some(EXTERNAL_DESC))
        .descriptor(KeychainKind::Internal, Some(INTERNAL_DESC))
        .extract_keys()
        .check_network(NETWORK)
        .load_wallet(&mut conn)?;
    let mut wallet = match wallet_opt {
        Some(wallet) => wallet,
        None => Wallet::create(EXTERNAL_DESC, INTERNAL_DESC)
            .network(NETWORK)
            .create_wallet(&mut conn)?,
    };

    let address = wallet.next_unused_address(KeychainKind::External);
    wallet.persist(&mut conn)?;
    println!("Next unused address: ({}) {}", address.index, address);

    let balance = wallet.balance();
    println!("Wallet balance before syncing: {}", balance.total());

    print!("Syncing...");
    let client = esplora_client::Builder::new(ESPLORA_URL).build_async()?;

    let request = wallet.start_full_scan().inspect({
        let mut stdout = std::io::stdout();
        let mut once = BTreeSet::<KeychainKind>::new();
        move |keychain, spk_i, _| {
            if once.insert(keychain) {
                print!("\nScanning keychain [{keychain:?}]");
            }
            print!(" {spk_i:<3}");
            stdout.flush().expect("must flush")
        }
    });

    let update = client
        .full_scan(request, STOP_GAP, PARALLEL_REQUESTS)
        .await?;

    wallet.apply_update(update)?;
    wallet.persist(&mut conn)?;
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

    let secp = bdk_wallet::bitcoin::key::Secp256k1::new();

    let (_, external_keymap) = Descriptor::parse_descriptor(&secp, EXTERNAL_DESC)?;
    let (_, internal_keymap) = Descriptor::parse_descriptor(&secp, INTERNAL_DESC)?;
    let key_map = external_keymap.into_iter().chain(internal_keymap).collect();

    // It's using the signer implementation from `test_utils`, you should implement your own signing
    // implementation for `KeyMap`.
    let signer = bdk_wallet::test_utils::SignerWrapper::new(key_map);
    let _ = psbt.sign(&signer, &secp);

    let finalized = wallet.finalize_psbt(&mut psbt, SignOptions::default())?;
    assert!(finalized);

    let tx = psbt.extract_tx()?;
    client.broadcast(&tx).await?;
    println!("Tx broadcasted! Txid: {}", tx.compute_txid());

    Ok(())
}

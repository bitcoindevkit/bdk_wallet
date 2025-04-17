#![allow(unused)]
use std::{collections::BTreeSet, io::Write};

use anyhow::Ok;
use bdk_esplora::{esplora_client, EsploraAsyncExt};
use bdk_testenv::bitcoincore_rpc::RpcApi;
use bdk_wallet::{
    bitcoin::{Amount, Network},
    rusqlite::Connection,
    KeychainKind, SignOptions, Wallet,
};

const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

const DB_PATH: &str = "bdk-example-esplora-async.sqlite";
// const NETWORK: Network = Network::Signet;
const NETWORK: Network = Network::Regtest;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ESPLORA_URL: &str = "http://signet.bitcoindevkit.net";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // let mut conn = Connection::open(DB_PATH)?;
    let mut conn = Connection::open_in_memory()?;

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

    // let address = wallet.next_unused_address(KeychainKind::External);
    let recv_addr = wallet.next_unused_address(KeychainKind::External);
    wallet.persist(&mut conn)?;
    // println!("Next unused address: ({}) {}", address.index, address);

    // let balance = wallet.balance();
    // println!("Wallet balance before syncing: {}", balance.total());

    // print!("Syncing...");
    // let client = esplora_client::Builder::new(ESPLORA_URL).build_async()?;

    // let request = wallet.start_full_scan().inspect({
    //     let mut stdout = std::io::stdout();
    //     let mut once = BTreeSet::<KeychainKind>::new();
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

    use bdk_testenv::bitcoincore_rpc::bitcoincore_rpc_json::CreateRawTransactionInput;
    use bdk_testenv::TestEnv;
    let env = TestEnv::new()?;

    // premine
    let rpc = env.rpc_client();
    let _ = env.mine_blocks(100, None);
    assert_eq!(rpc.get_block_count()?, 101);

    let utxo = rpc.list_unspent(None, None, None, None, None)?[0].clone();

    // Create tx1
    let utxos = vec![CreateRawTransactionInput {
        txid: utxo.txid,
        vout: utxo.vout,
        sequence: None,
    }];
    let to_send = Amount::ONE_BTC;
    let fee = Amount::from_sat(1_000);
    let change_addr = rpc.get_new_address(None, None)?.assume_checked();
    let out = [
        (recv_addr.to_string(), to_send),
        (change_addr.to_string(), utxo.amount - to_send - fee),
    ]
    .into();
    let tx = rpc.create_raw_transaction(&utxos, &out, None, None)?;
    let tx1 = rpc
        .sign_raw_transaction_with_wallet(&tx, None, None)?
        .transaction()?;

    // Create tx2 the double spend
    let new_addr = rpc.get_new_address(None, None)?.assume_checked();
    let out = [
        (new_addr.to_string(), to_send),
        (change_addr.to_string(), utxo.amount - to_send - (fee * 2)),
    ]
    .into();
    let tx = rpc.create_raw_transaction(&utxos, &out, None, None)?;
    let tx2 = rpc
        .sign_raw_transaction_with_wallet(&tx, None, None)?
        .transaction()?;

    // Sync after send tx 1
    let txid1 = rpc.send_raw_transaction(&tx1)?;
    println!("Send tx1 {}", txid1);

    let base_url = format!("http://{}", &env.electrsd.esplora_url.clone().unwrap());
    let client = esplora_client::Builder::new(base_url.as_str()).build_async()?;

    while client.get_height().await? < 101 {
        std::thread::sleep(std::time::Duration::from_millis(64));
    }
    env.wait_until_electrum_sees_txid(txid1, std::time::Duration::from_secs(10))?;

    let request = wallet.start_sync_with_revealed_spks();

    let resp = client.sync(request, PARALLEL_REQUESTS).await?;
    assert_eq!(resp.tx_update.txs.first().unwrap().compute_txid(), txid1);

    wallet.apply_update(resp)?;
    wallet.persist(&mut conn)?;

    assert_eq!(
        wallet.balance(wallet.include_unbroadcasted()).total(),
        Amount::ONE_BTC
    );
    println!(
        "Balance after send tx1: {}",
        wallet.balance(wallet.include_unbroadcasted()).total()
    );
    // We should expect tx1 to occur in a future sync
    let exp_spk_txids = wallet
        .tx_graph()
        .list_expected_spk_txids(
            wallet.local_chain(),
            wallet.local_chain().tip().block_id(),
            wallet.spk_index(),
            /*spk_index_range: */ ..,
        )
        .collect::<Vec<_>>();
    assert_eq!(
        exp_spk_txids.first(),
        Some(&(recv_addr.script_pubkey(), txid1))
    );

    // Now sync after send tx 2
    let txid2 = rpc.send_raw_transaction(&tx2)?;
    println!("Send tx2 {}", txid2);
    env.wait_until_electrum_sees_txid(txid2, std::time::Duration::from_secs(10))?;

    let request = wallet.start_sync_with_revealed_spks();

    let resp = client.sync(request, PARALLEL_REQUESTS).await?;
    assert!(resp.tx_update.txs.is_empty());
    assert!(resp
        .tx_update
        .evicted_ats
        .iter()
        .any(|&(txid, _)| txid == txid1));

    wallet.apply_update(resp)?;
    wallet.persist(&mut conn)?;

    println!(
        "Balance after send tx2: {}",
        wallet.balance(wallet.include_unbroadcasted()).total()
    );
    assert_eq!(
        wallet.balance(wallet.include_unbroadcasted()).total(),
        Amount::ZERO
    );

    // Load the persisted wallet
    {
        wallet = Wallet::load()
            .load_wallet(&mut conn)?
            .expect("wallet was persisted");

        // tx1 is there, but is not canonical
        assert!(wallet.tx_graph().full_txs().any(|node| node.txid == txid1));
        assert!(wallet
            .transactions(wallet.include_unbroadcasted())
            .next()
            .is_none());
        assert!(wallet
            .list_unspent(wallet.include_unbroadcasted())
            .next()
            .is_none());
        assert_eq!(
            wallet.balance(wallet.include_unbroadcasted()).total(),
            Amount::ZERO
        );
        println!(
            "Balance after load wallet: {}",
            wallet.balance(wallet.include_unbroadcasted()).total()
        );
    }

    Ok(())
}

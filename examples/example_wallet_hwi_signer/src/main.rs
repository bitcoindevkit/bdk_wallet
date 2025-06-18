use async_hwi::bitbox::api::runtime::TokioRuntime;
use async_hwi::bitbox::api::{usb, BitBox};
use async_hwi::bitbox::NoiseConfigNoCache;
use bdk_wallet::bitcoin::absolute::LockTime;
use bdk_wallet::bitcoin::hashes::Hash;
use bdk_wallet::bitcoin::{
    absolute, transaction, Amount, BlockHash, FeeRate, Network, OutPoint, Transaction, TxIn, TxOut,
};
use bdk_wallet::chain::{BlockId, TxUpdate};
use bdk_wallet::file_store::Store;
use bdk_wallet::Wallet;
use bdk_wallet::{KeychainKind, Update};
use std::sync::Arc;

use async_hwi::{bitbox::BitBox02, HWI};

const DB_MAGIC: &str = "bdk_wallet_hwi_signer";
const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const NETWORK: Network = Network::Regtest;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdfCLpvozodGytD3gRUa1M5WQz4kNuDZVf1inhcsSHXRpyLWN3k3Qy3nucrzz5hw2iZiEs6spehpee2WxqfSi31ByRJEu4rZ/84h/1h/0h/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdfCLpvozodGytD3gRUa1M5WQz4kNuDZVf1inhcsSHXRpyLWN3k3Qy3nucrzz5hw2iZiEs6spehpee2WxqfSi31ByRJEu4rZ/84h/1h/0h/1/*)";

pub fn new_tx(locktime: u32) -> Transaction {
    Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::from_consensus(locktime),
        input: vec![],
        output: vec![],
    }
}

pub fn insert_checkpoint(wallet: &mut Wallet, block: BlockId) {
    let mut cp = wallet.latest_checkpoint();
    cp = cp.insert(block);
    wallet
        .apply_update(Update {
            chain: Some(cp),
            ..Default::default()
        })
        .unwrap();
}

fn feed_wallet(wallet: &mut Wallet) {
    let sendto_address = wallet.next_unused_address(KeychainKind::External);
    let change_addr = wallet.next_unused_address(KeychainKind::Internal);

    let tx0 = Transaction {
        output: vec![TxOut {
            value: Amount::from_sat(76_000),
            script_pubkey: change_addr.script_pubkey(),
        }],
        ..new_tx(0)
    };

    let tx1 = Transaction {
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: tx0.compute_txid(),
                vout: 0,
            },
            ..Default::default()
        }],
        output: vec![
            TxOut {
                value: Amount::from_sat(50_000),
                script_pubkey: sendto_address.script_pubkey(),
            },
            TxOut {
                value: Amount::from_sat(25_000),
                script_pubkey: change_addr.script_pubkey(),
            },
        ],
        ..new_tx(0)
    };

    insert_checkpoint(
        wallet,
        BlockId {
            height: 42,
            hash: BlockHash::all_zeros(),
        },
    );
    insert_checkpoint(
        wallet,
        BlockId {
            height: 1_000,
            hash: BlockHash::all_zeros(),
        },
    );

    insert_checkpoint(
        wallet,
        BlockId {
            height: 2_000,
            hash: BlockHash::all_zeros(),
        },
    );

    let mut tx_update = TxUpdate::default();    
    let seen_at = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs();
    tx_update.seen_ats = [(tx0.compute_txid(), seen_at), (tx1.compute_txid(), seen_at)].into();
    tx_update.txs = vec![Arc::new(tx0), Arc::new(tx1)];


    wallet
        .apply_update(Update{tx_update, ..Default::default()})
        .unwrap();

}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let db_path = "bdk-signer-example.db";
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
 
    // Pairing with Bitbox connected Bitbox device
    let noise_config = Box::new(NoiseConfigNoCache {});

    let bitbox = {
        #[cfg(feature = "simulator")]
        {
            bitbox_api::BitBox::<TokioRuntime>::from_simulator(None, noise_config).await?
        }

        #[cfg(not(feature = "simulator"))]
        {
            BitBox::<TokioRuntime>::from_hid_device(usb::get_any_bitbox02().unwrap(), noise_config)
                .await?
        }
    };

    let pairing_device = bitbox.unlock_and_pair().await?;
    let paired_device = pairing_device.wait_confirm().await?;

    if let Ok(_) = paired_device.restore_from_mnemonic().await {
        println!("Initializing device with mnemonic...");
    } else {
        println!("Device already initialized proceeding...");
    }

    let bb = BitBox02::from(paired_device);
    let bb = bb.with_network(NETWORK);

    let receiving_wallet = wallet.next_unused_address(KeychainKind::External);

    feed_wallet(&mut wallet);

    println!("Wallet balance {}", wallet.balance());

    let mut tx_builder = wallet.build_tx();

    tx_builder
        .add_recipient(receiving_wallet.script_pubkey(), SEND_AMOUNT)
        .fee_rate(FeeRate::from_sat_per_vb(2).unwrap())
        .nlocktime(LockTime::from_height(0).unwrap());

    let mut psbt = tx_builder.finish()?;

    // Sign with the connected bitbox or any hardware device
    bb.sign_tx(&mut psbt).await?;

    println!("Signing with bitbox done");
    Ok(())
}

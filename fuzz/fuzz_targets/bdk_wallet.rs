#![no_main]

use libfuzzer_sys::fuzz_target;
use std::{
    cmp,
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
};

use bdk_wallet::{
    bitcoin::{hashes::Hash as _, BlockHash, Network, Txid},
    chain::{BlockId, ConfirmationBlockTime, TxUpdate},
    rusqlite::Connection,
    KeychainKind, Update, Wallet,
};

use bdk_wallet::bitcoin::{
    absolute::LockTime, transaction::Version, Amount, OutPoint, Transaction, TxIn, TxOut,
};

use bdk_wallet_fuzz::{
    fuzz_utils::*, try_consume_anchors, try_consume_bool, try_consume_byte, try_consume_checkpoint,
    try_consume_seen_or_evicted_ats, try_consume_txouts, try_consume_txs, try_consume_u32,
    try_consume_u64, try_consume_u8,
};

// descriptors
const INTERNAL_DESCRIPTOR: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const EXTERNAL_DESCRIPTOR: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";

// network
const NETWORK: Network = Network::Testnet;

pub enum WalletAction {
    ApplyUpdate,
    CreateTx,
    PersistAndLoad,
}

impl WalletAction {
    fn from_byte(byte: &u8) -> Option<WalletAction> {
        if *byte == 0x00 {
            Some(WalletAction::ApplyUpdate)
        } else if *byte == 0x01 {
            Some(WalletAction::CreateTx)
        } else if *byte == 0x02 {
            Some(WalletAction::PersistAndLoad)
        } else {
            None
        }
    }
}

fuzz_target!(|data: &[u8]| {
    // creates initial wallet.
    let mut db_conn = Connection::open_in_memory()
        .expect("Should start an in-memory database connection successfully!");
    let wallet = Wallet::create(EXTERNAL_DESCRIPTOR, INTERNAL_DESCRIPTOR)
        .network(NETWORK)
        .create_wallet(&mut db_conn);

    // asserts that the wallet creation did not fail.
    let mut wallet = match wallet {
        Ok(wallet) => wallet,
        Err(_) => return,
    };

    // fuzzed code goes here.
    let mut new_data = data;
    let mut data_iter = new_data.iter();
    while let Some(wallet_action) = WalletAction::from_byte(try_consume_byte!(data_iter)) {
        match wallet_action {
            WalletAction::ApplyUpdate => {
                // generated fuzzed keychain indices.
                let mut last_active_indices: BTreeMap<KeychainKind, u32> = BTreeMap::new();
                for keychain in [KeychainKind::Internal, KeychainKind::External] {
                    if try_consume_bool!(data_iter) {
                        let count = try_consume_u8!(data_iter) as u32;
                        let start = try_consume_u8!(data_iter) as u32;
                        last_active_indices.extend((start..count).map(|idx| (keychain, idx)))
                    }
                }

                // generate fuzzed tx update.
                let txs: Vec<std::sync::Arc<Transaction>> =
                    try_consume_txs!(&mut new_data, &mut wallet);

                let mut unconfirmed_txids: VecDeque<Txid> =
                    txs.iter().map(|tx| tx.compute_txid()).collect();

                let txouts = try_consume_txouts!(&mut new_data);
                let anchors = try_consume_anchors!(&mut new_data, unconfirmed_txids);
                let seen_ats = try_consume_seen_or_evicted_ats!(&mut new_data, unconfirmed_txids);
                let evicted_ats =
                    try_consume_seen_or_evicted_ats!(&mut new_data, unconfirmed_txids);

                // build the tx update with fuzzed data
                let mut tx_update = TxUpdate::default();
                tx_update.txs = txs;
                tx_update.txouts = txouts;
                tx_update.anchors = anchors;
                tx_update.seen_ats = seen_ats;
                tx_update.evicted_ats = evicted_ats;

                // generate fuzzed chain.
                let chain = try_consume_checkpoint!(&mut new_data, wallet);

                // apply fuzzed update.
                let update = Update {
                    last_active_indices,
                    tx_update,
                    chain: Some(chain),
                };

                wallet.apply_update(update).unwrap();
            }
            WalletAction::CreateTx => {
                // todo!()
                continue;
            }
            WalletAction::PersistAndLoad => {
                let expected_balance = wallet.balance();
                let expected_internal_index = wallet.next_derivation_index(KeychainKind::Internal);
                let expected_external_index = wallet.next_derivation_index(KeychainKind::External);
                let expected_tip = wallet.latest_checkpoint();
                let expected_genesis_hash =
                    BlockHash::from_byte_array(NETWORK.chain_hash().to_bytes());

                // generate fuzzed persist
                if let Err(e) = wallet.persist(&mut db_conn) {
                    assert!(
                        matches!(e, bdk_wallet::rusqlite::Error::ToSqlConversionFailure(..)),
                        "It should always persist successfully!"
                    );
                    return;
                };

                // generate fuzzed load
                wallet = Wallet::load()
                    .descriptor(KeychainKind::External, Some(EXTERNAL_DESCRIPTOR))
                    .descriptor(KeychainKind::Internal, Some(INTERNAL_DESCRIPTOR))
                    .check_network(NETWORK)
                    .check_genesis_hash(expected_genesis_hash)
                    .load_wallet(&mut db_conn)
                    .expect("It should always load from persistence successfully!")
                    .expect("It should load the wallet successfully!");

                // verify the persisted data is accurate
                assert_eq!(wallet.network(), NETWORK);
                assert_eq!(wallet.balance(), expected_balance);
                assert_eq!(
                    wallet.next_derivation_index(KeychainKind::Internal),
                    expected_internal_index
                );
                assert_eq!(
                    wallet.next_derivation_index(KeychainKind::External),
                    expected_external_index
                );
                assert_eq!(wallet.latest_checkpoint(), expected_tip);
            }
        }
    }
});

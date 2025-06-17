#![no_main]

use libfuzzer_sys::fuzz_target;
use std::{
    cmp,
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
};

use bdk_wallet::{
    bitcoin::{Network, Txid},
    chain::{BlockId, ConfirmationBlockTime, TxUpdate},
    descriptor::DescriptorError,
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
    let wallet: Result<Wallet, DescriptorError> =
        Wallet::create(INTERNAL_DESCRIPTOR, EXTERNAL_DESCRIPTOR)
            .network(NETWORK)
            .create_wallet_no_persist();

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
                // todo!()
                continue;
            }
        }
    }
});

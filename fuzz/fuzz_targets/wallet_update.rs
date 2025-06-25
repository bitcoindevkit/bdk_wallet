#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::{BTreeMap, VecDeque};

use bdk_wallet::{
    bitcoin::{Network, Txid},
    chain::TxUpdate,
    descriptor::DescriptorError,
    KeychainKind, Update, Wallet,
};
use bdk_wallet_fuzz::fuzz_utils::*;

// descriptors
const INTERNAL_DESCRIPTOR: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const EXTERNAL_DESCRIPTOR: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";

// network
const NETWORK: Network = Network::Testnet;

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

    // generated fuzzed keychain indices.
    let internal_indices = consume_keychain_indices(&mut new_data, KeychainKind::Internal);
    let external_indices = consume_keychain_indices(&mut new_data, KeychainKind::External);

    let mut last_active_indices: BTreeMap<KeychainKind, u32> = BTreeMap::new();
    last_active_indices.extend(internal_indices);
    last_active_indices.extend(external_indices);

    // generate fuzzed tx update.
    let txs = consume_txs(data, &mut wallet);

    let unconfirmed_txids: VecDeque<Txid> = txs.iter().map(|tx| tx.compute_txid()).collect();

    let txouts = consume_txouts(data);
    let anchors = consume_anchors(data, unconfirmed_txids.clone());
    let seen_ats = consume_seen_ats(data, unconfirmed_txids.clone());
    let evicted_ats = consume_evicted_ats(data, unconfirmed_txids.clone());

    // build the tx update with fuzzed data
    let mut tx_update = TxUpdate::default();
    tx_update.txs = txs;
    tx_update.txouts = txouts;
    tx_update.anchors = anchors;
    tx_update.seen_ats = seen_ats;
    tx_update.evicted_ats = evicted_ats;

    // generate fuzzed chain.
    let chain = consume_checkpoint(data, &mut wallet);

    // apply fuzzed update.
    let update = Update {
        last_active_indices,
        tx_update,
        chain: Some(chain),
    };

    wallet.apply_update(update).unwrap();
});

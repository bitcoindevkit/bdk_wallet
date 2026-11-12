use std::collections::BTreeMap;

use bdk_wallet::{
    KeychainKind, Wallet,
    chain::BlockId,
    test_utils::{get_test_wpkh_and_change_desc, insert_checkpoint, insert_tx_anchor, new_tx},
};
use bitcoin::{
    Amount, BlockHash, Network, OutPoint, ScriptBuf, Transaction, TxIn, TxOut, Txid, hashes::Hash,
};

fn wallet() -> Wallet {
    let (external, internal) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(external, internal)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();
    wallet.reveal_next_address(KeychainKind::External);
    wallet.reveal_next_address(KeychainKind::Internal);
    wallet.reveal_next_address(KeychainKind::Internal);
    wallet
}

fn block(height: u32) -> BlockId {
    BlockId {
        height,
        hash: BlockHash::from_byte_array([height as u8; 32]),
    }
}

fn script(wallet: &Wallet, keychain: KeychainKind, index: u32) -> ScriptBuf {
    wallet.peek_address(keychain, index).address.script_pubkey()
}

fn receive(spk: ScriptBuf, nonce: u32, outputs: usize) -> Transaction {
    let mut tx = new_tx(nonce);
    tx.output = (0..outputs)
        .map(|_| TxOut {
            value: Amount::from_sat(10_000),
            script_pubkey: spk.clone(),
        })
        .collect();
    tx
}

fn spend(outpoints: &[OutPoint], nonce: u32) -> Transaction {
    let mut tx = new_tx(nonce);
    tx.input = outpoints
        .iter()
        .map(|&previous_output| TxIn {
            previous_output,
            ..Default::default()
        })
        .collect();
    tx.output.push(TxOut {
        value: Amount::from_sat(1_000),
        script_pubkey: ScriptBuf::new(),
    });
    tx
}

fn sync(wallet: &Wallet, confirmations: Option<u32>) -> BTreeMap<ScriptBuf, Vec<Txid>> {
    let mut request = wallet
        .start_sync_with_pending_spks_at(123, confirmations)
        .build();
    assert_eq!(request.start_time(), 123);
    assert_eq!(request.chain_tip().unwrap(), wallet.latest_checkpoint());
    request
        .iter_spks_with_expected_txids()
        .map(|item| {
            let mut txids: Vec<_> = item.expected_txids.into_iter().collect();
            txids.sort();
            (item.spk, txids)
        })
        .collect()
}

fn settled(wallet: &mut Wallet, keychain: KeychainKind) -> (ScriptBuf, Transaction, Transaction) {
    let spk = script(wallet, keychain, 0);
    let recv = receive(spk.clone(), 1, 1);
    let sent = spend(&[OutPoint::new(recv.compute_txid(), 0)], 2);
    insert_tx_anchor(wallet, recv.clone(), block(10));
    insert_tx_anchor(wallet, sent.clone(), block(11));
    insert_checkpoint(wallet, block(16));
    (spk, recv, sent)
}

#[test]
fn opt_in_retains_external_unused_and_unspent_scripts() {
    let mut wallet = wallet();
    let (external, _, _) = settled(&mut wallet, KeychainKind::External);
    let internal = script(&wallet, KeychainKind::Internal, 0);
    insert_tx_anchor(&mut wallet, receive(internal.clone(), 3, 1), block(12));
    let requested = sync(&wallet, None);
    assert!(requested.contains_key(&external));
    assert!(requested.contains_key(&internal));
    assert!(requested.contains_key(&script(&wallet, KeychainKind::Internal, 1)));
    assert_eq!(requested.len(), 3);
}

#[test]
fn default_six_confirmation_boundary_and_existing_api_are_preserved() {
    let mut wallet = wallet();
    let (spk, _, _) = settled(&mut wallet, KeychainKind::Internal);
    assert!(!sync(&wallet, None).contains_key(&spk)); // Six confirmations at tip 16.
    assert!(sync(&wallet, Some(7)).contains_key(&spk));
    assert!(!sync(&wallet, Some(6)).contains_key(&spk));
    let mut original = wallet.start_sync_with_revealed_spks_at(123).build();
    assert!(
        original
            .iter_spks_with_expected_txids()
            .any(|item| item.spk == spk)
    );
    let mut wallet = self::wallet();
    let recv = receive(spk.clone(), 1, 1);
    let sent = spend(&[OutPoint::new(recv.compute_txid(), 0)], 2);
    insert_tx_anchor(&mut wallet, recv, block(10));
    insert_tx_anchor(&mut wallet, sent, block(11));
    insert_checkpoint(&mut wallet, block(15));
    assert!(sync(&wallet, None).contains_key(&spk)); // Five confirmations.
    insert_checkpoint(&mut wallet, block(16));
    assert!(!sync(&wallet, None).contains_key(&spk));
}

#[test]
fn all_outputs_must_be_spent_even_with_multiple_outputs_to_one_script() {
    let mut wallet = wallet();
    let spk = script(&wallet, KeychainKind::Internal, 0);
    let recv = receive(spk.clone(), 1, 2);
    let txid = recv.compute_txid();
    insert_tx_anchor(&mut wallet, recv, block(10));
    insert_tx_anchor(&mut wallet, spend(&[OutPoint::new(txid, 0)], 2), block(11));
    insert_checkpoint(&mut wallet, block(16));
    assert!(sync(&wallet, None).contains_key(&spk));
    insert_tx_anchor(&mut wallet, spend(&[OutPoint::new(txid, 1)], 3), block(11));
    assert!(!sync(&wallet, None).contains_key(&spk));
}

#[test]
fn unconfirmed_replacement_and_eviction_keep_script_and_expected_txids() {
    let mut wallet = wallet();
    let spk = script(&wallet, KeychainKind::Internal, 0);
    let recv = receive(spk.clone(), 1, 1);
    let prevout = OutPoint::new(recv.compute_txid(), 0);
    insert_tx_anchor(&mut wallet, recv, block(10));
    insert_checkpoint(&mut wallet, block(16));
    let original = spend(&[prevout], 2);
    let replacement = spend(&[prevout], 3);
    wallet.apply_unconfirmed_txs([(original.clone(), 20)]);
    assert!(sync(&wallet, Some(0))[&spk].contains(&original.compute_txid()));
    wallet.apply_unconfirmed_txs([(replacement.clone(), 21)]);
    let requested = sync(&wallet, None);
    assert!(requested[&spk].contains(&replacement.compute_txid()));
    assert!(!requested[&spk].contains(&original.compute_txid()));
    wallet.apply_evicted_txs([
        (replacement.compute_txid(), 22),
        (original.compute_txid(), 22),
    ]);
    assert!(sync(&wallet, None).contains_key(&spk));
}

#[test]
fn reorg_and_newly_learned_reuse_reactivate_internal_script() {
    let mut wallet = wallet();
    let (spk, _, sent) = settled(&mut wallet, KeychainKind::Internal);
    assert!(!sync(&wallet, None).contains_key(&spk));
    // Replace the spend's anchoring block; the old transaction becomes unconfirmed.
    insert_checkpoint(
        &mut wallet,
        BlockId {
            height: 11,
            hash: BlockHash::from_byte_array([99; 32]),
        },
    );
    wallet.apply_unconfirmed_txs([(sent, 20)]);
    assert!(sync(&wallet, None).contains_key(&spk));

    let mut wallet = self::wallet();
    let (spk, _, _) = settled(&mut wallet, KeychainKind::Internal);
    wallet.apply_unconfirmed_txs([(receive(spk.clone(), 4, 1), 20)]);
    assert!(sync(&wallet, None).contains_key(&spk));
}

#[test]
fn partial_previous_output_history_cannot_look_settled() {
    let mut wallet = wallet();
    let (spk, _, _) = settled(&mut wallet, KeychainKind::Internal);
    let unknown = OutPoint::new(Txid::from_byte_array([42; 32]), 0);
    wallet.insert_txout(
        unknown,
        TxOut {
            value: Amount::from_sat(10_000),
            script_pubkey: spk.clone(),
        },
    );
    insert_tx_anchor(&mut wallet, spend(&[unknown], 5), block(11));
    assert!(sync(&wallet, None).contains_key(&spk));
}

#[test]
fn single_descriptor_wallet_keeps_every_revealed_script() {
    let (external, _) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create_single(external)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();
    wallet.reveal_next_address(KeychainKind::Internal);
    let (spk, _, _) = settled(&mut wallet, KeychainKind::External);
    assert!(sync(&wallet, None).contains_key(&spk));
    let mut timed = wallet.start_sync_with_pending_spks(None).build();
    assert!(
        timed
            .iter_spks_with_expected_txids()
            .any(|item| item.spk == spk)
    );
}

#[test]
fn floating_receive_after_settled_history_keeps_internal_script() {
    let mut wallet = wallet();
    let (spk, _, _) = settled(&mut wallet, KeychainKind::Internal);
    assert!(!sync(&wallet, None).contains_key(&spk));
    wallet.insert_txout(
        OutPoint::new(Txid::from_byte_array([43; 32]), 0),
        TxOut {
            value: Amount::from_sat(10_000),
            script_pubkey: spk.clone(),
        },
    );
    assert!(sync(&wallet, None).contains_key(&spk));
}

#[test]
fn confirmed_replacement_does_not_keep_internal_script_pending() {
    let mut wallet = wallet();
    let spk = script(&wallet, KeychainKind::Internal, 0);
    let recv = receive(spk.clone(), 1, 1);
    let prevout = OutPoint::new(recv.compute_txid(), 0);
    insert_tx_anchor(&mut wallet, recv, block(10));
    let original = spend(&[prevout], 2);
    wallet.apply_unconfirmed_txs([(original, 20)]);
    let replacement = spend(&[prevout], 3);
    insert_tx_anchor(&mut wallet, replacement, block(11));
    insert_checkpoint(&mut wallet, block(16));
    assert!(!sync(&wallet, None).contains_key(&spk));
}

#[test]
fn settled_internal_history_is_omitted_from_large_request() {
    let mut wallet = wallet();
    wallet
        .reveal_addresses_to(KeychainKind::External, 2)
        .for_each(drop);
    wallet
        .reveal_addresses_to(KeychainKind::Internal, 34)
        .for_each(drop);
    for index in 0..32 {
        let spk = script(&wallet, KeychainKind::Internal, index);
        let recv = receive(spk, 100 + index * 2, 1);
        let sent = spend(&[OutPoint::new(recv.compute_txid(), 0)], 101 + index * 2);
        insert_tx_anchor(&mut wallet, recv, block(10));
        insert_tx_anchor(&mut wallet, sent, block(11));
    }
    for index in 32..34 {
        let spk = script(&wallet, KeychainKind::Internal, index);
        insert_tx_anchor(&mut wallet, receive(spk, 300 + index, 1), block(10));
    }
    insert_checkpoint(&mut wallet, block(16));
    let mut original = wallet.start_sync_with_revealed_spks_at(123).build();
    assert_eq!(original.iter_spks_with_expected_txids().count(), 38);
    let requested = sync(&wallet, None);
    assert_eq!(requested.len(), 6);
    for index in 0..3 {
        assert!(requested.contains_key(&script(&wallet, KeychainKind::External, index)));
    }
    for index in 32..35 {
        assert!(requested.contains_key(&script(&wallet, KeychainKind::Internal, index)));
    }
}

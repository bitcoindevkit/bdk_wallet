use std::str::FromStr;

use assert_matches::assert_matches;
use bdk_wallet::coin_selection::InsufficientFunds;
use bdk_wallet::error::CreateTxError;
use bdk_wallet::test_utils::*;
use bdk_wallet::{KeychainKind, Wallet};
use bitcoin::{
    absolute, transaction, Address, Amount, OutPoint, SignedAmount, Transaction, TxIn, TxOut, Txid,
};

mod common;

fn get_tx_for_min_output_value_test(wallet: &mut Wallet, funded_txid: Txid) -> Transaction {
    let small_outpoint = receive_output_in_latest_block(wallet, Amount::from_sat(600));
    let wallet_change = wallet.next_unused_address(KeychainKind::External);
    let recipient = Address::from_str("bcrt1q3qtze4ys45tgdvguj66zrk4fu6hq3a3v9pfly5")
        .expect("address")
        .assume_checked();

    Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::ZERO,
        input: vec![
            TxIn {
                previous_output: OutPoint::new(funded_txid, 0),
                ..Default::default()
            },
            TxIn {
                previous_output: small_outpoint,
                ..Default::default()
            },
        ],
        output: vec![
            TxOut {
                script_pubkey: wallet_change.script_pubkey(),
                value: Amount::from_sat(700),
            },
            TxOut {
                script_pubkey: recipient.script_pubkey(),
                value: Amount::from_sat(50_000),
            },
        ],
    }
}

#[test]
fn test_min_output_value_list_unspent() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let initial_count = wallet.list_unspent().count();

    receive_output_in_latest_block(&mut wallet, Amount::from_sat(600));
    assert_eq!(wallet.list_unspent().count(), initial_count + 1);

    wallet.set_min_output_value(Some(Amount::from_sat(1_000)));

    let unspent: Vec<_> = wallet.list_unspent().collect();
    assert_eq!(unspent.len(), initial_count);
    assert!(unspent
        .iter()
        .all(|u| u.txout.value >= Amount::from_sat(1_000)));
}

#[test]
fn test_min_output_value_list_output() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    receive_output_in_latest_block(&mut wallet, Amount::from_sat(600));

    let all_count = wallet.list_output().count();

    wallet.set_min_output_value(Some(Amount::from_sat(1_000)));

    let filtered: Vec<_> = wallet.list_output().collect();
    assert!(filtered.len() < all_count);
    assert!(filtered
        .iter()
        .all(|o| o.txout.value >= Amount::from_sat(1_000)));
}

#[test]
fn test_min_output_value_balance() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let initial_balance = wallet.balance().confirmed;

    receive_output_in_latest_block(&mut wallet, Amount::from_sat(600));
    assert_eq!(
        wallet.balance().confirmed,
        initial_balance + Amount::from_sat(600)
    );

    // Set threshold
    wallet.set_min_output_value(Some(Amount::from_sat(1_000)));

    assert_eq!(wallet.balance().confirmed, initial_balance);
}

#[test]
fn test_create_params_min_output_value_sets_wallet_value() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let wallet = Wallet::create(desc, change_desc)
        .network(bitcoin::Network::Regtest)
        .min_output_value(Amount::from_sat(1_000))
        .create_wallet_no_persist()
        .expect("descriptors must be valid");

    assert_eq!(wallet.min_output_value(), Some(Amount::from_sat(1_000)));
}

#[test]
fn test_load_params_min_output_value_sets_wallet_value() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    receive_output_in_latest_block(&mut wallet, Amount::from_sat(600));
    let changeset = wallet
        .take_staged()
        .expect("wallet should have a staged changeset");

    let wallet = Wallet::load()
        .min_output_value(Amount::from_sat(1_000))
        .load_wallet_no_persist(changeset)
        .expect("changeset should be valid")
        .expect("changeset should load a wallet");

    assert_eq!(wallet.min_output_value(), Some(Amount::from_sat(1_000)));
}

#[test]
fn test_set_min_output_value_can_be_cleared() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let initial_balance = wallet.balance().confirmed;
    receive_output_in_latest_block(&mut wallet, Amount::from_sat(600));

    wallet.set_min_output_value(Some(Amount::from_sat(1_000)));
    assert_eq!(wallet.min_output_value(), Some(Amount::from_sat(1_000)));
    assert_eq!(wallet.balance().confirmed, initial_balance);

    wallet.set_min_output_value(None);

    assert_eq!(wallet.min_output_value(), None);
    assert_eq!(
        wallet.balance().confirmed,
        initial_balance + Amount::from_sat(600)
    );
}

#[test]
fn test_min_output_value_sent_and_received() {
    let (mut wallet, funded_txid) = get_funded_wallet_wpkh();
    let tx = get_tx_for_min_output_value_test(&mut wallet, funded_txid);

    assert_eq!(
        wallet.sent_and_received(&tx),
        (Amount::from_sat(50_600), Amount::from_sat(700))
    );

    wallet.set_min_output_value(Some(Amount::from_sat(1_000)));

    assert_eq!(
        wallet.sent_and_received(&tx),
        (Amount::from_sat(50_000), Amount::ZERO)
    );
}

#[test]
fn test_min_output_value_net_value() {
    let (mut wallet, funded_txid) = get_funded_wallet_wpkh();
    let tx = get_tx_for_min_output_value_test(&mut wallet, funded_txid);

    assert_eq!(wallet.net_value(&tx), SignedAmount::from_sat(-49_900));

    wallet.set_min_output_value(Some(Amount::from_sat(1_000)));

    assert_eq!(wallet.net_value(&tx), SignedAmount::from_sat(-50_000));
}

#[test]
fn test_min_output_value_coin_selection() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _, _) = new_wallet_and_funding_update(desc, Some(change_desc));
    receive_output(&mut wallet, Amount::from_sat(1_500), ReceiveTo::Mempool(1));

    let recipient = Address::from_str("bcrt1q3qtze4ys45tgdvguj66zrk4fu6hq3a3v9pfly5")
        .expect("address")
        .assume_checked();

    let mut baseline_builder = wallet.build_tx();
    baseline_builder.add_recipient(recipient.script_pubkey(), Amount::from_sat(1_000));
    assert!(
        baseline_builder.finish().is_ok(),
        "wallet should spend the small UTXO before the threshold is set"
    );

    wallet.set_min_output_value(Some(Amount::from_sat(2_000)));

    let mut filtered_builder = wallet.build_tx();
    filtered_builder.add_recipient(recipient.script_pubkey(), Amount::from_sat(1_000));

    assert_matches!(
        filtered_builder.finish(),
        Err(CreateTxError::CoinSelection(InsufficientFunds {
            available: Amount::ZERO,
            ..
        }))
    );
}

#[test]
fn test_min_output_value_coin_selection_ignores_small_utxos_when_large_utxos_exist() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let small_outpoint = receive_output_in_latest_block(&mut wallet, Amount::from_sat(600));
    let recipient = Address::from_str("bcrt1q3qtze4ys45tgdvguj66zrk4fu6hq3a3v9pfly5")
        .expect("address")
        .assume_checked();

    let mut baseline_builder = wallet.build_tx();
    baseline_builder
        .drain_to(recipient.script_pubkey())
        .drain_wallet();
    let baseline_psbt = baseline_builder.finish().unwrap();

    assert!(
        baseline_psbt
            .unsigned_tx
            .input
            .iter()
            .any(|txin| txin.previous_output == small_outpoint),
        "drain_wallet should include the small UTXO before the threshold is set"
    );

    wallet.set_min_output_value(Some(Amount::from_sat(1_000)));

    let mut filtered_builder = wallet.build_tx();
    filtered_builder
        .drain_to(recipient.script_pubkey())
        .drain_wallet();
    let filtered_psbt = filtered_builder.finish().unwrap();

    assert!(
        filtered_psbt
            .unsigned_tx
            .input
            .iter()
            .all(|txin| txin.previous_output != small_outpoint),
        "coin selection should ignore the small UTXO after applying the threshold"
    );
}

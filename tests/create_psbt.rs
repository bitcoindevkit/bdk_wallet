//! Integration tests for the unstable `create_psbt` and `replace_by_fee` APIs.
#![cfg(all(bdk_wallet_unstable, feature = "bdk-tx"))]

use bdk_chain::{BlockId, ConfirmationBlockTime};
use bdk_tx::{bdk_coin_select, ChangeScript};
use bdk_wallet::bitcoin;
use bdk_wallet::test_utils::*;
use bdk_wallet::{
    error::CreatePsbtError, psbt, KeychainKind, PsbtParams, SelectionStrategy, Wallet,
};
use bitcoin::{
    absolute, hashes::Hash, Amount, FeeRate, Network, OutPoint, ScriptBuf, Sequence, Transaction,
    TxIn, TxOut,
};
use miniscript::plan::Assets;

// Test that `create_psbt` results in the expected PSBT.
#[test]
fn test_create_psbt() {
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();
    let expected_xpub = match wallet.public_descriptor(KeychainKind::External) {
        miniscript::Descriptor::Tr(tr) => match tr.internal_key() {
            miniscript::DescriptorPublicKey::XPub(desc) => desc.xkey,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };

    // Receive coins
    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 100,
            hash: Hash::hash(b"100"),
        },
        confirmation_time: 1234567000,
    };
    insert_checkpoint(&mut wallet, anchor.block_id);
    receive_output(&mut wallet, Amount::ONE_BTC, ReceiveTo::Block(anchor));

    let change_descriptor = wallet
        .public_descriptor(KeychainKind::Internal)
        .at_derivation_index(0)
        .unwrap();

    let addr = wallet.reveal_next_address(KeychainKind::External);
    let mut params = PsbtParams::default();
    let feerate = FeeRate::from_sat_per_vb(4).unwrap();
    let selection_strategy = psbt::SelectionStrategy::LowestFee {
        longterm_feerate: FeeRate::from_sat_per_vb(2).unwrap(),
        max_rounds: 1000,
    };
    params
        .version(bitcoin::transaction::Version(3))
        .coin_selection(selection_strategy)
        .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())])
        .change_script(ChangeScript::from_descriptor(change_descriptor))
        .fee_rate(feerate)
        .add_global_xpubs();

    let (psbt, _) = wallet.create_psbt(params).unwrap();
    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.version.0, 3);
    assert_eq!(tx.lock_time.to_consensus_u32(), 0);
    assert_eq!(tx.input.len(), 1);
    assert_eq!(tx.output.len(), 2);

    // global xpubs
    assert_eq!(
        psbt.xpub,
        [(expected_xpub, ("f6a5cb8b".parse().unwrap(), vec![].into()))].into(),
    );
    // witness utxo
    let psbt_input = &psbt.inputs[0];
    assert_eq!(
        psbt_input.witness_utxo.as_ref().map(|txo| txo.value),
        Some(Amount::ONE_BTC),
    );
    // input internal key
    assert!(psbt_input.tap_internal_key.is_some());
    // input key origins
    assert!(psbt_input
        .tap_key_origins
        .values()
        .any(|(_, (fp, _))| fp.to_string() == "f6a5cb8b"));
    // output internal key
    assert!(psbt
        .outputs
        .iter()
        .any(|output| output.tap_internal_key.is_some()));
    // output key origins
    assert!(psbt.outputs.iter().any(|output| output
        .tap_key_origins
        .values()
        .any(|(_, (fp, _))| fp.to_string() == "f6a5cb8b")));
}

#[test]
fn test_create_psbt_insufficient_funds_error() {
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let addr = wallet.reveal_next_address(KeychainKind::External);

    let mut params = PsbtParams::default();
    params.add_recipients([(addr.script_pubkey(), Amount::from_sat(10_000))]);

    let result = wallet.create_psbt(params);
    assert!(matches!(
        result,
        Err(CreatePsbtError::InsufficientFunds(
            bdk_coin_select::InsufficientFunds { missing: 10_000 }
        )),
    ));
}

#[test]
fn test_create_psbt_maturity_height() {
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();
    let receive_address = wallet.reveal_next_address(KeychainKind::External);
    let send_to_address = wallet.reveal_next_address(KeychainKind::External).address;

    let block_1 = BlockId {
        height: 1,
        hash: Hash::hash(b"1"),
    };
    insert_checkpoint(&mut wallet, block_1);

    // Receive coinbase output at height = 1.
    // maturity height = (1 + 100) = 101
    let tx = Transaction {
        input: vec![TxIn::default()],
        output: vec![TxOut {
            value: Amount::ONE_BTC,
            script_pubkey: receive_address.script_pubkey(),
        }],
        ..new_tx(0)
    };
    insert_tx_anchor(&mut wallet, tx, block_1);

    // The output is still immature at height = 99.
    let mut p = PsbtParams::default();
    p.add_recipients([(send_to_address.clone(), Amount::from_sat(58_000))])
        .maturity_height(bitcoin::absolute::Height::from_consensus(99).unwrap());

    let _ = wallet
        .create_psbt(p)
        .expect_err("immature output must not be selected");

    // We can use the params to coerce the coinbase maturity.
    let mut p = PsbtParams::default();
    p.add_recipients([(send_to_address.clone(), Amount::from_sat(58_000))])
        .maturity_height(bitcoin::absolute::Height::from_consensus(100).unwrap());

    let _ = wallet
        .create_psbt(p)
        .expect("`maturity_height` should enable selection");

    // The output is eligible for selection once the wallet tip reaches maturity height minus 1
    // (100), as it can be confirmed in the next block (101).
    let block_100 = BlockId {
        height: 100,
        hash: Hash::hash(b"100"),
    };
    insert_checkpoint(&mut wallet, block_100);
    let mut p = PsbtParams::default();
    p.add_recipients([(send_to_address.clone(), Amount::from_sat(58_000))]);

    let _ = wallet
        .create_psbt(p)
        .expect("mature coinbase should be selected");
}

#[test]
fn test_create_psbt_cltv() {
    use absolute::LockTime;

    let desc = get_test_single_sig_cltv();
    let mut wallet = Wallet::create_single(desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    // Receive coins
    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 99_999,
            hash: Hash::hash(b"abc"),
        },
        confirmation_time: 1234567000,
    };
    insert_checkpoint(&mut wallet, anchor.block_id);
    let op = receive_output(&mut wallet, Amount::ONE_BTC, ReceiveTo::Block(anchor));

    let addr = wallet.reveal_next_address(KeychainKind::External);

    // No assets fail
    {
        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);
        let res = wallet.create_psbt(params);
        assert!(
            matches!(res, Err(CreatePsbtError::Plan(err)) if err == op),
            "UTXO requires CLTV but the assets are insufficient",
        );
    }

    // Add assets ok
    {
        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .add_assets(Assets::new().after(LockTime::from_consensus(100_000)))
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);
        let (psbt, _) = wallet.create_psbt(params).unwrap();
        assert_eq!(psbt.unsigned_tx.lock_time.to_consensus_u32(), 100_000);
    }

    // New chain tip (no assets) ok
    {
        let block_id = BlockId {
            height: 100_000,
            hash: Hash::hash(b"123"),
        };
        insert_checkpoint(&mut wallet, block_id);

        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);
        let (psbt, _) = wallet.create_psbt(params).unwrap();
        assert_eq!(psbt.unsigned_tx.lock_time.to_consensus_u32(), 100_000);
    }

    // Locktime greater than required
    {
        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .locktime(LockTime::from_consensus(200_000))
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);

        let (psbt, _) = wallet.create_psbt(params).unwrap();
        assert_eq!(psbt.unsigned_tx.lock_time.to_consensus_u32(), 200_000);
    }
}

#[test]
fn test_create_psbt_cltv_timestamp() {
    use absolute::LockTime;

    let lock_time = LockTime::from_consensus(1734230218);
    let desc = get_test_single_sig_cltv_timestamp();
    let mut wallet = Wallet::create_single(desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    // Receive coins
    let op = receive_output(&mut wallet, Amount::ONE_BTC, ReceiveTo::Mempool(1));

    let addr = wallet.reveal_next_address(KeychainKind::External);

    // No assets fail
    {
        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);
        let res = wallet.create_psbt(params);
        assert!(
            matches!(res, Err(CreatePsbtError::Plan(err)) if err == op),
            "UTXO requires CLTV but the assets are insufficient",
        );
    }

    // Add assets ok
    {
        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .add_assets(Assets::new().after(lock_time))
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);
        let (psbt, _) = wallet.create_psbt(params).unwrap();
        assert_eq!(psbt.unsigned_tx.lock_time, lock_time);
    }

    // Locktime greater than required
    {
        let new_lock_time = 1772167108;
        assert!(new_lock_time > lock_time.to_consensus_u32());
        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .add_assets(Assets::new().after(lock_time))
            .locktime(LockTime::from_consensus(new_lock_time))
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);

        let (psbt, _) = wallet.create_psbt(params).unwrap();
        assert_eq!(psbt.unsigned_tx.lock_time.to_consensus_u32(), new_lock_time);
    }
}

#[test]
fn test_create_psbt_csv() {
    use bitcoin::relative;
    use bitcoin::Sequence;

    let desc = get_test_single_sig_csv();
    let mut wallet = Wallet::create_single(desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    // Receive coins
    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 10_000,
            hash: Hash::hash(b"abc"),
        },
        confirmation_time: 1234567000,
    };
    insert_checkpoint(&mut wallet, anchor.block_id);
    let op = receive_output(&mut wallet, Amount::ONE_BTC, ReceiveTo::Block(anchor));

    let addr = wallet.reveal_next_address(KeychainKind::External);

    // No assets fail
    {
        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);
        let res = wallet.create_psbt(params);
        assert!(
            matches!(res, Err(CreatePsbtError::Plan(err)) if err == op),
            "UTXO requires CSV but the assets are insufficient",
        );
    }

    // Add assets ok
    {
        let mut params = PsbtParams::default();
        let rel_locktime = relative::LockTime::from_consensus(6).unwrap();
        params
            .add_utxos(&[op])
            .add_assets(Assets::new().older(rel_locktime))
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);
        let (psbt, _) = wallet.create_psbt(params).unwrap();
        assert_eq!(psbt.unsigned_tx.input[0].sequence, Sequence(6));
    }

    // Add 6 confirmations (no assets)
    {
        let anchor = ConfirmationBlockTime {
            block_id: BlockId {
                height: 10_005,
                hash: Hash::hash(b"xyz"),
            },
            confirmation_time: 1234567000,
        };
        insert_checkpoint(&mut wallet, anchor.block_id);
        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);
        let (psbt, _) = wallet.create_psbt(params).unwrap();
        assert_eq!(psbt.unsigned_tx.input[0].sequence, Sequence(6));
    }
}

/// Fallback sequence is applied to a coin-selected input that has no CSV
/// requirement.
#[test]
fn test_create_psbt_fallback_sequence_applied_to_coin_selected_input() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut params = PsbtParams::default();
    params
        .add_recipients([(addr.script_pubkey(), Amount::from_sat(25_000))])
        .fallback_sequence(Sequence::ENABLE_RBF_NO_LOCKTIME);
    let psbt = wallet.create_psbt(params).unwrap().0;
    assert_eq!(
        psbt.unsigned_tx.input[0].sequence,
        Sequence::ENABLE_RBF_NO_LOCKTIME
    );
}

/// Fallback sequence is NOT applied when the input already has a CSV-derived sequence
/// requirement — the CSV value wins.
#[test]
fn test_create_psbt_fallback_sequence_skipped_for_csv_input() {
    use bitcoin::relative;
    let mut wallet = Wallet::create_single(get_test_single_sig_csv())
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();
    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 10_000,
            hash: Hash::hash(b"csv_fallback"),
        },
        confirmation_time: 1_234_567_000,
    };
    insert_checkpoint(&mut wallet, anchor.block_id);
    let op = receive_output(
        &mut wallet,
        Amount::from_sat(100_000),
        ReceiveTo::Block(anchor),
    );

    let addr = wallet.next_unused_address(KeychainKind::External);
    let rel_locktime = relative::LockTime::from_consensus(6).unwrap();
    let mut params = PsbtParams::default();
    params
        .add_utxos(&[op])
        .add_assets(Assets::new().older(rel_locktime))
        .add_recipients([(addr.script_pubkey(), Amount::from_sat(25_000))])
        .fallback_sequence(Sequence::ENABLE_RBF_NO_LOCKTIME);
    let psbt = wallet.create_psbt(params).unwrap().0;
    // CSV descriptor requires older(6); fallback must not clobber the CSV-derived sequence.
    assert_eq!(psbt.unsigned_tx.input[0].sequence, Sequence(6));
}

/// A per-input sequence override is applied to a manually-selected UTXO.
#[test]
fn test_create_psbt_sequence_override_manually_selected_input() {
    let (mut wallet, txid) = get_funded_wallet_wpkh();
    let utxo = OutPoint::new(txid, 0);
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut params = PsbtParams::default();
    params
        .add_recipients([(addr.script_pubkey(), Amount::from_sat(25_000))])
        .add_utxos(&[utxo])
        .manually_selected_only()
        .sequence_override(utxo, Sequence(42));
    let psbt = wallet.create_psbt(params).unwrap().0;
    assert_eq!(psbt.unsigned_tx.input[0].sequence, Sequence(42));
}

/// A per-input sequence override takes precedence over a fallback sequence.
#[test]
fn test_create_psbt_sequence_override_takes_precedence_over_fallback() {
    let (mut wallet, txid) = get_funded_wallet_wpkh();
    let utxo = OutPoint::new(txid, 0);
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut params = PsbtParams::default();
    params
        .add_recipients([(addr.script_pubkey(), Amount::from_sat(25_000))])
        .add_utxos(&[utxo])
        .manually_selected_only()
        .sequence_override(utxo, Sequence(42))
        .fallback_sequence(Sequence::ENABLE_RBF_NO_LOCKTIME);
    let psbt = wallet.create_psbt(params).unwrap().0;
    assert_eq!(psbt.unsigned_tx.input[0].sequence, Sequence(42));
}

/// A sequence override that violates the CSV requirement returns a Sequence error.
#[test]
fn test_create_psbt_sequence_override_csv_conflict_returns_error() {
    use bitcoin::relative;
    let mut wallet = Wallet::create_single(get_test_single_sig_csv())
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();
    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 10_000,
            hash: Hash::hash(b"csv_override"),
        },
        confirmation_time: 1_234_567_000,
    };
    insert_checkpoint(&mut wallet, anchor.block_id);
    let op = receive_output(
        &mut wallet,
        Amount::from_sat(100_000),
        ReceiveTo::Block(anchor),
    );

    let addr = wallet.next_unused_address(KeychainKind::External);
    let rel_locktime = relative::LockTime::from_consensus(6).unwrap();
    let mut params = PsbtParams::default();
    params
        .add_utxos(&[op])
        .add_assets(Assets::new().older(rel_locktime))
        .add_recipients([(addr.script_pubkey(), Amount::from_sat(25_000))])
        .manually_selected_only()
        .sequence_override(op, Sequence(3)); // CSV requires >= 6
    let result = wallet.create_psbt(params);
    assert!(matches!(result, Err(CreatePsbtError::Sequence(_))));
}

// Test that replacing tx A also accounts for the fees of A's unconfirmed descendants
// B and C when calculating the minimum required replacement fee (RBF Rule 3).
//
//   A       A'
//  / \
// B   C
//
// A' conflicts with A. The replacement fee should exceed
// fee(A) + fee(B) + fee(C).
#[test]
fn test_replace_by_fee_replaces_descendant_fees() {
    use KeychainKind::*;

    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let block_id = BlockId {
        height: 100,
        hash: Hash::hash(b"100"),
    };

    // addr0 receives the confirmed funding; addr1 and addr2 are wallet change
    // addresses that tx A pays into so that B and C can spend them.
    let addr0 = wallet.reveal_next_address(External).address;
    let addr1 = wallet.reveal_next_address(Internal).address;
    let addr2 = wallet.reveal_next_address(Internal).address;

    // External (non-wallet) output script used as a sink for recipients.
    let external =
        ScriptBuf::from_hex("5120e8f5c4dc2f5d6a7595e7b108cb063da9c7550312da1e22875d78b9db62b59cd5")
            .unwrap();

    // Confirmed funding tx: 1_000_000 sats to addr0.
    let funding_tx = Transaction {
        input: vec![TxIn::default()],
        output: vec![TxOut {
            value: Amount::from_sat(1_000_000),
            script_pubkey: addr0.script_pubkey(),
        }],
        ..new_tx(0)
    };
    let funding_op = OutPoint::new(funding_tx.compute_txid(), 0);
    insert_tx_anchor(&mut wallet, funding_tx.clone(), block_id);

    // Tx A (unconfirmed): spends the confirmed UTXO; two outputs return to wallet.
    //   fee_a = 1_000_000 - 50_000 - 450_000 - 450_000 = 50_000 sats
    let tx_a = Transaction {
        input: vec![TxIn {
            previous_output: funding_op,
            ..TxIn::default()
        }],
        output: vec![
            TxOut {
                value: Amount::from_sat(50_000),
                script_pubkey: external.clone(),
            },
            TxOut {
                value: Amount::from_sat(450_000),
                script_pubkey: addr1.script_pubkey(),
            },
            TxOut {
                value: Amount::from_sat(450_000),
                script_pubkey: addr2.script_pubkey(),
            },
        ],
        ..new_tx(0)
    };
    let a_txid = tx_a.compute_txid();
    let fee_a = wallet.calculate_fee(&tx_a).unwrap();
    insert_tx(&mut wallet, tx_a.clone());

    // Tx B (unconfirmed): spends A's first change output.
    //   fee_b = 450_000 - 430_000 = 20_000 sats
    let tx_b = Transaction {
        input: vec![TxIn {
            previous_output: OutPoint::new(a_txid, 1),
            ..TxIn::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(430_000),
            script_pubkey: external.clone(),
        }],
        ..new_tx(0)
    };
    let fee_b = wallet.calculate_fee(&tx_b).unwrap();
    insert_tx(&mut wallet, tx_b);

    // Tx C (unconfirmed): spends A's second change output.
    //   fee_c = 450_000 - 430_000 = 20_000 sats
    let tx_c = Transaction {
        input: vec![TxIn {
            previous_output: OutPoint::new(a_txid, 2),
            ..TxIn::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(430_000),
            script_pubkey: external.clone(),
        }],
        ..new_tx(0)
    };
    let fee_c = wallet.calculate_fee(&tx_c).unwrap();
    insert_tx(&mut wallet, tx_c.clone());

    // The replacement must pay at least the combined fee of all three transactions
    // (Bitcoin Core RBF Rule 3).
    let total_original_fee = fee_a + fee_b + fee_c;
    assert_eq!(total_original_fee.to_sat(), 90_000);

    // Build replacement A'. The wallet walks A's descendants (B and C) so their
    // fees are included in the minimum required replacement fee.
    let mut params = PsbtParams::default();
    params.add_recipients([(external, Amount::from_sat(100_000))]);
    params.fee_rate(FeeRate::from_sat_per_vb(4).unwrap());
    let params = params.replace_txs([tx_a]);
    let (psbt, _) = wallet
        .replace_by_fee(params)
        .expect("failed to create RBF PSBT");

    let replacement_fee = wallet
        .calculate_fee(&psbt.unsigned_tx)
        .expect("replacement tx fee should be calculable");

    assert!(
        replacement_fee >= total_original_fee,
        "replacement fee ({replacement_fee}) must be >= sum of fees for A + B + C ({total_original_fee})",
    );
}

// Test that `replace_by_fee`` rejects a confirmed original tx
#[test]
fn test_replace_by_fee_confirmed_tx_error() {
    use bdk_wallet::error::ReplaceByFeeError;
    use KeychainKind::*;

    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let block = BlockId {
        height: 100,
        hash: Hash::hash(b"100"),
    };
    let addr = wallet.reveal_next_address(External).address;

    // Fund the wallet with a confirmed output.
    let funding_tx = Transaction {
        input: vec![TxIn::default()],
        output: vec![TxOut {
            value: Amount::from_sat(200_000),
            script_pubkey: addr.script_pubkey(),
        }],
        ..new_tx(0)
    };
    let funding_op = OutPoint::new(funding_tx.compute_txid(), 0);
    insert_tx_anchor(&mut wallet, funding_tx, block);

    // Create an unconfirmed tx spending the confirmed UTXO.
    let recip =
        ScriptBuf::from_hex("5120e8f5c4dc2f5d6a7595e7b108cb063da9c7550312da1e22875d78b9db62b59cd5")
            .unwrap();
    let mut params = PsbtParams::default();
    params
        .add_utxos(&[funding_op])
        .add_recipients([(recip.clone(), Amount::from_sat(100_000))]);
    let unconfirmed_tx = wallet.create_psbt(params).unwrap().0.unsigned_tx;
    insert_tx(&mut wallet, unconfirmed_tx.clone());

    // Now confirm that tx.
    let confirmed_txid = unconfirmed_tx.compute_txid();
    let confirm_block = BlockId {
        height: 1001,
        hash: Hash::hash(b"1001"),
    };
    insert_tx_anchor(&mut wallet, unconfirmed_tx.clone(), confirm_block);

    // Attempting to replace the now-confirmed tx should return TransactionConfirmed.
    let mut params = PsbtParams::default();
    params.add_recipients([(recip, Amount::from_sat(10_000))]);
    params.fee_rate(FeeRate::from_sat_per_vb(10).unwrap());
    let params = params.replace_txs([unconfirmed_tx]);
    let result = wallet.replace_by_fee(params);

    assert!(
        matches!(result, Err(ReplaceByFeeError::TransactionConfirmed(txid)) if txid == confirmed_txid),
        "expected TransactionConfirmed error, got: {result:?}",
    );
}

// Test that `replace_by_fee` errors when all original inputs have been removed via
// `remove_utxo`, leaving the replacement with no inputs from the replaced transaction.
#[test]
fn test_replace_by_fee_no_inputs_from_original() {
    use bdk_wallet::error::ReplaceByFeeError;
    use KeychainKind::*;

    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let addr = wallet.reveal_next_address(External).address;

    // Fund the wallet with an unconfirmed output.
    let funding_tx = Transaction {
        input: vec![TxIn {
            previous_output: OutPoint::new(Hash::hash(b"funding_parent"), 0),
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(200_000),
            script_pubkey: addr.script_pubkey(),
        }],
        ..new_tx(0)
    };
    let funding_op = OutPoint::new(funding_tx.compute_txid(), 0);
    insert_tx(&mut wallet, funding_tx);

    // Create an unconfirmed tx spending the funded UTXO.
    let recip =
        ScriptBuf::from_hex("5120e8f5c4dc2f5d6a7595e7b108cb063da9c7550312da1e22875d78b9db62b59cd5")
            .unwrap();
    let mut params = PsbtParams::default();
    params
        .add_utxos(&[funding_op])
        .add_recipients([(recip.clone(), Amount::from_sat(100_000))]);
    let unconfirmed_tx = wallet.create_psbt(params).unwrap().0.unsigned_tx;
    let unconfirmed_txid = unconfirmed_tx.compute_txid();
    insert_tx(&mut wallet, unconfirmed_tx.clone());

    // Build replacement params with a recipient but remove the original inputs.
    let mut params = PsbtParams::default().replace_txs([unconfirmed_tx]);
    params
        .remove_utxo(&funding_op)
        .add_recipients([(recip, Amount::from_sat(50_000))]);

    let result = wallet.replace_by_fee(params);
    assert!(
        matches!(result, Err(ReplaceByFeeError::NoInputsFromOriginal(txid)) if txid == unconfirmed_txid),
        "expected NoInputsFromOriginal error, got: {result:?}",
    );
}

// Test that `replace_by_fee` returns `NoOriginalTransactions` when `replace_txs` is called
// with an empty list, i.e. no transactions were provided for replacement.
#[test]
fn test_replace_by_fee_no_original_transactions() {
    use bdk_wallet::error::ReplaceByFeeError;

    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    // replace_txs with an empty iterator produces PsbtParams<ReplaceTx> with an empty replace set.
    let params = PsbtParams::default().replace_txs(core::iter::empty::<Transaction>());
    let result = wallet.replace_by_fee(params);
    assert!(
        matches!(result, Err(ReplaceByFeeError::NoOriginalTransactions)),
        "expected NoOriginalTransactions, got: {result:?}",
    );
}

// Test that `replace_by_fee` rejects a manually-selected input that spends
// from a descendant of the one being replaced.
#[test]
fn test_replace_by_fee_conflicting_input_descendant() {
    use bdk_tx::Input as BdkInput;
    use bdk_wallet::error::ReplaceByFeeError;
    use bitcoin::{psbt as btc_psbt, Sequence};

    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let addr = wallet.reveal_next_address(KeychainKind::External).address;

    // Fund the wallet so there is a spendable UTXO.
    let funding_tx = Transaction {
        input: vec![TxIn {
            previous_output: OutPoint::new(Hash::hash(b"funding_parent"), 0),
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(500_000),
            script_pubkey: addr.script_pubkey(),
        }],
        ..new_tx(0)
    };
    let funding_op = OutPoint::new(funding_tx.compute_txid(), 0);
    insert_tx(&mut wallet, funding_tx);

    let recip =
        ScriptBuf::from_hex("5120e8f5c4dc2f5d6a7595e7b108cb063da9c7550312da1e22875d78b9db62b59cd5")
            .unwrap();

    // tx_parent: the transaction we will eventually replace.
    let mut params = PsbtParams::default();
    params
        .add_utxos(&[funding_op])
        .add_recipients([(recip.clone(), Amount::from_sat(100_000))]);
    let tx_parent = wallet.create_psbt(params).unwrap().0.unsigned_tx;
    let txid_parent = tx_parent.compute_txid();
    insert_tx(&mut wallet, tx_parent.clone());

    // tx_child: spends one of tx_parent's outputs (a descendant of the tx being replaced).
    let child_output = TxOut {
        value: Amount::from_sat(10_000),
        script_pubkey: ScriptBuf::new_p2wpkh(
            &bitcoin::WPubkeyHash::from_slice(&[0u8; 20]).unwrap(),
        ),
    };
    let child_op = OutPoint::new(txid_parent, 0);
    let tx_child = Transaction {
        input: vec![TxIn {
            previous_output: child_op,
            ..Default::default()
        }],
        output: vec![child_output.clone()],
        ..new_tx(1)
    };
    let txid_child = tx_child.compute_txid();
    insert_tx(&mut wallet, tx_child.clone());

    // A planned input that spends an output of tx_child (a descendant of the replaced tx).
    // This is the indirect conflict that params-level stripping cannot catch.
    let grandchild_op = OutPoint::new(txid_child, 0);
    let grandchild_input = BdkInput::from_psbt_input(
        grandchild_op,
        Sequence::ENABLE_RBF_NO_LOCKTIME,
        btc_psbt::Input {
            witness_utxo: Some(child_output),
            ..Default::default()
        },
        /* satisfaction_weight */ 0,
        /* status */ None,
        /* is_coinbase */ false,
        /* absolute_timelock */ None,
    )
    .unwrap();

    // Build replacement for tx_parent, adding the grandchild planned input.
    let mut params = PsbtParams::default();
    params.add_planned_input(grandchild_input);
    params.add_recipients([(recip, Amount::from_sat(50_000))]);
    let params = params.replace_txs([tx_parent]);

    let result = wallet.replace_by_fee(params);
    assert!(
        matches!(result, Err(ReplaceByFeeError::ConflictingInput(op)) if op == grandchild_op),
        "expected ConflictingInput({grandchild_op}), got: {result:?}",
    );
}

#[test]
fn test_create_psbt_utxo_filter() {
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 1000,
            hash: Hash::hash(b"1000"),
        },
        confirmation_time: 1234567,
    };
    insert_checkpoint(&mut wallet, anchor.block_id);

    for value in [200, 300, 600, 1000] {
        let _ = receive_output(
            &mut wallet,
            Amount::from_sat(value),
            ReceiveTo::Block(anchor),
        );
    }
    assert_eq!(wallet.list_unspent().count(), 4);
    assert_eq!(wallet.balance(1).total().to_sat(), 2100);

    let mut params = PsbtParams::default();
    params.fee_rate(FeeRate::ZERO);
    // Avoid selection of dust utxos
    params.utxo_filter(|txo| {
        let min_non_dust = txo.txout.script_pubkey.minimal_non_dust(); // 330
        txo.txout.value >= min_non_dust
    });
    let change_script = ChangeScript::from_descriptor(
        wallet
            .public_descriptor(KeychainKind::Internal)
            .at_derivation_index(0)
            .unwrap(),
    );
    params.change_script(change_script);
    params.coin_selection(psbt::SelectionStrategy::All);
    let (psbt, _) = wallet.create_psbt(params).unwrap();
    assert_eq!(psbt.unsigned_tx.input.len(), 2);
    assert_eq!(psbt.unsigned_tx.output.len(), 1);
    assert_eq!(
        psbt.unsigned_tx.output[0].value.to_sat(),
        1600,
        "We should have selected 2 non-dust utxos"
    );
}

// Verify that `create_psbt` returns `NoRecipients` when no recipients are provided and
// `drain_wallet` is not set, even when the wallet contains multiple UTXOs.
#[test]
fn test_create_psbt_no_recipients_error() {
    use bdk_chain::{BlockId, ConfirmationBlockTime};
    use bdk_wallet::error::CreatePsbtError;

    let (mut wallet, _) = get_funded_wallet_wpkh();

    // Add a second confirmed UTXO so we can confirm it's not "just draining one".
    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 200,
            hash: bitcoin::hashes::Hash::hash(b"200"),
        },
        confirmation_time: 2000,
    };
    insert_checkpoint(&mut wallet, anchor.block_id);
    receive_output(&mut wallet, bitcoin::Amount::from_sat(25_000), anchor);

    // No recipients, no drain_wallet → should error.
    let err = wallet.create_psbt(PsbtParams::default()).unwrap_err();
    assert!(
        matches!(err, CreatePsbtError::NoRecipients),
        "expected NoRecipients, got {err:?}"
    );

    // drain_wallet with an explicit change_script and no recipients should succeed (sweep to
    // change).
    let mut params = PsbtParams::default();
    let change_descriptor = wallet
        .public_descriptor(KeychainKind::Internal)
        .at_derivation_index(0)
        .unwrap();
    params
        .coin_selection(SelectionStrategy::All)
        .change_script(ChangeScript::from_descriptor(change_descriptor));
    wallet
        .create_psbt(params)
        .expect("drain_wallet with explicit change_script should succeed");
}

// When `drain_wallet` is set but the only output (change) would fall below the dust threshold,
// verify that `create_psbt` surfaces this as an error rather than returning a zero-output
// PSBT.
#[test]
fn test_create_psbt_drain_wallet_change_below_dust_error() {
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 100,
            hash: Hash::hash(b"100"),
        },
        confirmation_time: 0,
    };
    insert_checkpoint(&mut wallet, anchor.block_id);

    // 200 sats: enough to meet the minimum fee for a P2TR spend,
    // but after deducting fees for a tx that *includes* a change output the
    // residual change falls below the dust threshold.
    receive_output(&mut wallet, Amount::from_sat(200), ReceiveTo::Block(anchor));

    let change_descriptor = wallet
        .public_descriptor(KeychainKind::Internal)
        .at_derivation_index(0)
        .unwrap();
    let mut params = PsbtParams::default();
    params
        .coin_selection(SelectionStrategy::All)
        .change_script(ChangeScript::from_descriptor(change_descriptor));

    let err = wallet.create_psbt(params).unwrap_err();
    assert!(
        matches!(err, CreatePsbtError::AllOutputsBelowDust),
        "expected AllOutputsBelowDust when change is below dust threshold, got {err:?}"
    );
}

// Same dust-drop edge case but via `replace_by_fee`. When `drain_wallet` is set
// and the only output (change) falls below dust, the resulting transaction
// would have zero outputs. Verify that `replace_by_fee` returns the expected error.
#[test]
fn test_replace_by_fee_drain_wallet_change_below_dust_error() {
    use bdk_wallet::error::ReplaceByFeeError;
    use bitcoin::transaction;

    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 100,
            hash: Hash::hash(b"100"),
        },
        confirmation_time: 0,
    };
    insert_checkpoint(&mut wallet, anchor.block_id);

    // 400 sats: at 1 sat/vb, fees for a P2TR tx with 1 input + 1 change output ≈ 111 sat,
    // leaving ~289 sat change — below the P2TR dust threshold (~303 sat).
    let op = receive_output(&mut wallet, Amount::from_sat(400), ReceiveTo::Block(anchor));

    // Build an original unconfirmed tx that spends `op` with RBF enabled.
    let original_tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: op,
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(300),
            script_pubkey: wallet
                .peek_address(KeychainKind::External, 1)
                .script_pubkey(),
        }],
    };
    insert_tx(&mut wallet, original_tx.clone());

    // RBF with `drain_wallet`, no recipients, default fee rate (1 sat/vb).
    // The only possible output (change) falls below dust.
    let change_descriptor = wallet
        .public_descriptor(KeychainKind::Internal)
        .at_derivation_index(0)
        .unwrap();
    let mut params = PsbtParams::default().replace_txs([original_tx]);
    params
        .coin_selection(SelectionStrategy::All)
        .change_script(ChangeScript::from_descriptor(change_descriptor));
    let err = wallet.replace_by_fee(params).unwrap_err();
    assert!(
        matches!(
            err,
            ReplaceByFeeError::CreatePsbt(CreatePsbtError::AllOutputsBelowDust)
        ),
        "expected AllOutputsBelowDust when RBF change is below dust threshold, got {err:?}"
    );
}

#[test]
fn test_replace_tx_with_planned_input() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let addr = wallet.reveal_next_address(KeychainKind::External).address;

    // Fund the wallet with an unconfirmed output.
    let funding_tx = Transaction {
        input: vec![TxIn {
            previous_output: OutPoint::new(Hash::hash(b"funding_parent"), 0),
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(200_000),
            script_pubkey: addr.script_pubkey(),
        }],
        ..new_tx(0)
    };
    let funding_op = OutPoint::new(funding_tx.compute_txid(), 0);
    insert_tx(&mut wallet, funding_tx);

    // Create an unconfirmed tx spending the funded UTXO.
    let recip =
        ScriptBuf::from_hex("5120e8f5c4dc2f5d6a7595e7b108cb063da9c7550312da1e22875d78b9db62b59cd5")
            .unwrap();
    let op2 = OutPoint::new(Hash::hash(b"txid"), 2);
    let txout = TxOut {
        value: Amount::ZERO,
        script_pubkey: ScriptBuf::new_p2a(),
    };
    wallet.insert_txout(op2, txout.clone());
    let psbt_input = bitcoin::psbt::Input {
        witness_utxo: Some(txout),
        ..Default::default()
    };
    let planned_input = bdk_tx::Input::from_psbt_input(
        op2,
        Sequence::ENABLE_LOCKTIME_NO_RBF,
        psbt_input,
        /* satisfaction_weight: */ 0,
        /* status: */ None,
        /* is_coinbase: */ false,
        /* absolute_timelock: */ None,
    )
    .unwrap();

    let mut params = PsbtParams::default();
    params
        .add_utxos(&[funding_op])
        .add_planned_input(planned_input.clone())
        .add_recipients([(recip.clone(), Amount::from_sat(100_000))]);
    let unconfirmed_tx = wallet.create_psbt(params).unwrap().0.unsigned_tx;
    insert_tx(&mut wallet, unconfirmed_tx.clone());

    // Add the planned input *before* calling replace_txs. The replace() method
    // should respect pre-registered planned inputs in the unique set.
    let mut params = PsbtParams::default();
    params
        .add_planned_input(planned_input.clone())
        .add_recipients([(recip, Amount::from_sat(99_000))]);
    let params = params.replace_txs([unconfirmed_tx]);

    let (psbt, _) = wallet
        .replace_by_fee(params)
        .expect("replacement should succeed");
    assert_eq!(
        psbt.unsigned_tx.input.len(),
        2,
        "replacement tx must include both the wallet input and the planned input"
    );
    assert!(
        psbt.unsigned_tx
            .input
            .iter()
            .any(|txin| txin.previous_output == funding_op),
        "replacement must include the wallet-controlled input"
    );
    assert!(
        psbt.unsigned_tx
            .input
            .iter()
            .any(|txin| txin.previous_output == op2),
        "replacement must include the planned input"
    );
}

#[test]
fn test_add_planned_psbt_input() -> anyhow::Result<()> {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let op1 = wallet.list_unspent().next().unwrap().outpoint;

    // We'll use `PsbtParams` to sweep a foreign anchor output.
    let op2 = OutPoint::new(Hash::hash(b"txid"), 2);
    let txout = TxOut {
        value: Amount::ZERO,
        script_pubkey: ScriptBuf::new_p2a(),
    };
    let psbt_input = bitcoin::psbt::Input {
        witness_utxo: Some(txout),
        ..Default::default()
    };
    let input = bdk_tx::Input::from_psbt_input(
        op2,
        Sequence::ENABLE_LOCKTIME_NO_RBF,
        psbt_input,
        /* satisfaction_weight: */ 0,
        /* status: */ None,
        /* is_coinbase: */ false,
        /* absolute_timelock: */ None,
    )?;

    let send_to = wallet.reveal_next_address(KeychainKind::External).address;

    // Build tx: 2-in / 2-out
    let mut params = PsbtParams::default();
    params.add_utxos(&[op1]);
    params.add_planned_input(input);
    params.add_recipients([(send_to, Amount::from_sat(20_000))]);

    let (psbt, _) = wallet.create_psbt(params)?;

    assert!(
        psbt.unsigned_tx
            .input
            .iter()
            .any(|input| input.previous_output == op1),
        "Psbt should contain the wallet spend"
    );
    assert!(
        psbt.unsigned_tx
            .input
            .iter()
            .any(|input| input.previous_output == op2),
        "Psbt should contain the planned input"
    );

    Ok(())
}

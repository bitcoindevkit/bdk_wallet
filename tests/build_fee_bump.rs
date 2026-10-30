use std::str::FromStr;

use assert_matches::assert_matches;
use bdk_chain::{ChainPosition, ConfirmationBlockTime};
use bdk_wallet::KeychainKind;
use bdk_wallet::coin_selection::LargestFirstCoinSelection;
use bdk_wallet::error::CreateTxError;
use bdk_wallet::psbt::PsbtUtils;
use bdk_wallet::test_utils::*;
use bitcoin::{
    Address, Amount, FeeRate, OutPoint, ScriptBuf, Sequence, Transaction, TxOut, Weight, absolute,
    hashes::Hash, psbt, transaction,
};

mod common;
use common::*;

/// The transaction of the `psbt` with a P2WPKH witness added to every input, as if it was signed.
fn fake_signed_tx(psbt: &psbt::Psbt) -> Transaction {
    let mut tx = psbt.unsigned_tx.clone();
    for txin in &mut tx.input {
        txin.witness.push([0x00; P2WPKH_FAKE_SIG_SIZE]);
        txin.witness.push([0x00; P2WPKH_FAKE_PK_SIZE]);
    }
    tx
}

#[test]
#[should_panic(expected = "IrreplaceableTransaction")]
fn test_bump_fee_irreplaceable_tx() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx();
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(25_000));
    builder.set_exact_sequence(Sequence(0xFFFFFFFE));
    let psbt = builder.finish().unwrap();

    let tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);
    wallet.build_fee_bump(txid).unwrap().finish().unwrap();
}

#[test]
#[should_panic(expected = "TransactionConfirmed")]
fn test_bump_fee_confirmed_tx() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx();
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(25_000));
    let psbt = builder.finish().unwrap();

    let tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();

    insert_tx(&mut wallet, tx);

    let anchor = ConfirmationBlockTime {
        block_id: wallet.latest_checkpoint().get(42).unwrap().block_id(),
        confirmation_time: 42_000,
    };
    insert_anchor(&mut wallet, txid, anchor);

    wallet.build_fee_bump(txid).unwrap().finish().unwrap();
}

#[test]
fn test_bump_fee_low_fee_rate() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx();
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(25_000));

    let psbt = builder.finish().unwrap();
    let feerate = psbt.fee_rate().unwrap();

    let tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_rate(FeeRate::BROADCAST_MIN);
    let res = builder.finish();
    assert_matches!(
        res,
        Err(CreateTxError::FeeRateTooLow { .. }),
        "expected FeeRateTooLow error"
    );

    let required = feerate.to_sat_per_kwu() + 1;
    let sat_vb = required as f64 / 250.0;
    let expect = format!("Fee rate too low: required {sat_vb} sat/vb");
    assert_eq!(res.unwrap_err().to_string(), expect);
}

#[test]
#[should_panic(expected = "FeeTooLow")]
fn test_bump_fee_low_abs() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx();
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(25_000));
    let psbt = builder.finish().unwrap();

    let tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_absolute(Amount::from_sat(10));
    builder.finish().unwrap();
}

#[test]
#[should_panic(expected = "FeeTooLow")]
fn test_bump_fee_zero_abs() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx();
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(25_000));
    let psbt = builder.finish().unwrap();

    let tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_absolute(Amount::ZERO);
    builder.finish().unwrap();
}

#[test]
fn test_bump_fee_absolute_equal() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx();
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(25_000));
    let psbt = builder.finish().unwrap();
    let fee = check_fee!(wallet, psbt);

    let tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_absolute(fee);
    assert_matches!(
        builder.finish(),
        Err(CreateTxError::FeeTooLow { required }) if required == fee
    );
}

#[test]
fn test_bump_fee_absolute_lower_fee_rate() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let incoming_op = receive_output_in_latest_block(&mut wallet, Amount::from_sat(25_000));
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx().coin_selection(LargestFirstCoinSelection);
    builder
        .add_recipient(addr.script_pubkey(), Amount::from_sat(25_000))
        .fee_rate(FeeRate::from_sat_per_vb_u32(50));
    let psbt = builder.finish().unwrap();
    let fee = check_fee!(wallet, psbt);

    let tx = fake_signed_tx(&psbt);
    let txid = tx.compute_txid();
    let required_feerate = FeeRate::from_sat_per_kwu((fee / tx.weight()).to_sat_per_kwu() + 1);
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .add_utxo(incoming_op)
        .unwrap()
        .fee_absolute(fee + Amount::from_sat(1_000));
    let Err(CreateTxError::FeeRateTooLow { required }) = builder.finish() else {
        panic!("expected FeeRateTooLow error");
    };

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.add_utxo(incoming_op).unwrap().fee_absolute(fee * 3);
    let weight = fake_signed_tx(&builder.finish().unwrap()).weight();
    let required_fee = required_feerate * Weight::from_wu(weight.to_wu().next_multiple_of(4));
    // The fee rate reported as required is enough at the weight of the replacement.
    assert!(required * weight >= required_fee);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .add_utxo(incoming_op)
        .unwrap()
        .fee_absolute(required_fee - Amount::from_sat(1));
    assert_matches!(builder.finish(), Err(CreateTxError::FeeRateTooLow { .. }));

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .add_utxo(incoming_op)
        .unwrap()
        .fee_absolute(required_fee);
    let psbt = builder.finish().unwrap();
    assert_eq!(psbt.fee_amount(), Some(required_fee));
    assert!(required_fee / fake_signed_tx(&psbt).weight() >= required_feerate);
}

#[test]
fn test_bump_fee_absolute_incremental_relay_fee() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx();
    builder
        .add_recipient(addr.script_pubkey(), Amount::from_sat(25_000))
        .fee_rate(FeeRate::from_sat_per_vb_u32(50));
    let psbt = builder.finish().unwrap();
    let fee = check_fee!(wallet, psbt);

    let tx = fake_signed_tx(&psbt);
    let txid = tx.compute_txid();
    let original_weight = tx.weight();
    let required_feerate = FeeRate::from_sat_per_kwu((fee / original_weight).to_sat_per_kwu() + 1);
    insert_tx(&mut wallet, tx);

    // Sweeping the input to a single output makes the replacement smaller than the original.
    let mut bump = |fee: Amount| {
        let mut builder = wallet.build_fee_bump(txid).unwrap();
        builder
            .set_recipients(Vec::new())
            .drain_to(addr.script_pubkey())
            .fee_absolute(fee);
        builder.finish()
    };

    let weight = fake_signed_tx(&bump(fee + Amount::from_sat(1_000)).unwrap()).weight();
    assert!(weight < original_weight);

    // The replacement must pay the incremental relay fee for its whole size on top of the fee of
    // the original
    let required_fee = fee + FeeRate::BROADCAST_MIN * weight;

    let below = required_fee - Amount::from_sat(1);
    assert!(below / weight >= required_feerate);
    assert_matches!(
        bump(below),
        Err(CreateTxError::FeeTooLow { required }) if required == required_fee
    );

    let psbt = bump(required_fee).unwrap();
    assert_eq!(psbt.fee_amount(), Some(required_fee));
}

#[test]
fn test_bump_fee_rate_incremental_relay_fee() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx();
    builder
        .add_recipient(addr.script_pubkey(), Amount::from_sat(25_000))
        .fee_rate(FeeRate::from_sat_per_vb_u32(50));
    let psbt = builder.finish().unwrap();
    let fee = check_fee!(wallet, psbt);

    let tx = fake_signed_tx(&psbt);
    let txid = tx.compute_txid();
    let original_weight = tx.weight();
    let lowest_feerate = FeeRate::from_sat_per_kwu((fee / original_weight).to_sat_per_kwu() + 1);
    insert_tx(&mut wallet, tx);

    let mut bump = |feerate: FeeRate| {
        let mut builder = wallet.build_fee_bump(txid).unwrap();
        builder
            .set_recipients(Vec::new())
            .drain_to(addr.script_pubkey())
            .fee_rate(feerate);
        builder.finish()
    };

    let weight = fake_signed_tx(&bump(FeeRate::from_sat_per_vb_u32(100)).unwrap()).weight();
    assert!(weight < original_weight);

    let required_fee = fee + FeeRate::BROADCAST_MIN * weight;
    let feerate = FeeRate::from_sat_per_kwu(lowest_feerate.to_sat_per_kwu() + 100);
    assert!(feerate * weight < fee);
    assert_matches!(
        bump(feerate),
        Err(CreateTxError::FeeTooLow { required }) if required == required_fee
    );

    let feerate =
        FeeRate::from_sat_per_kwu((required_fee.to_sat() * 1000).div_ceil(weight.to_wu()));
    let psbt = bump(feerate).unwrap();
    assert!(psbt.fee_amount().unwrap() >= required_fee);
}

#[test]
fn test_bump_fee_absolute_descendant_fee() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx();
    builder
        .add_recipient(addr.script_pubkey(), Amount::from_sat(25_000))
        .fee_rate(FeeRate::from_sat_per_vb_u32(50));
    let psbt = builder.finish().unwrap();
    let fee = check_fee!(wallet, psbt);

    let tx = fake_signed_tx(&psbt);
    let txid = tx.compute_txid();
    let change_vout = tx
        .output
        .iter()
        .position(|txout| txout.script_pubkey != addr.script_pubkey())
        .unwrap();
    insert_tx(&mut wallet, tx);

    // A child spends the change of the original, so it is evicted from the mempool when the
    // original is replaced.
    let mut builder = wallet.build_tx();
    builder
        .manually_selected_only()
        .add_utxo(OutPoint::new(txid, change_vout as u32))
        .unwrap()
        .add_recipient(addr.script_pubkey(), Amount::from_sat(10_000))
        .fee_rate(FeeRate::from_sat_per_vb_u32(30));
    let child = fake_signed_tx(&builder.finish().unwrap());
    let descendant_fee = wallet.calculate_fee(&child).unwrap();
    insert_tx(&mut wallet, child);

    let mut bump = |fee: Amount| {
        let mut builder = wallet.build_fee_bump(txid).unwrap();
        builder.fee_absolute(fee);
        builder.finish()
    };
    let weight =
        fake_signed_tx(&bump(fee + descendant_fee + Amount::from_sat(1_000)).unwrap()).weight();

    // The replacement pays for the original and its descendant
    let required_fee = fee + descendant_fee + FeeRate::BROADCAST_MIN * weight;
    assert_matches!(
        bump(required_fee - Amount::from_sat(1)),
        Err(CreateTxError::FeeTooLow { required }) if required == required_fee
    );
    assert!(bump(required_fee).is_ok());
}

#[test]
fn test_bump_fee_reduce_change() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx();
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(25_000));
    let psbt = builder.finish().unwrap();
    let original_sent_received =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let original_fee = check_fee!(wallet, psbt);

    let tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let feerate = FeeRate::from_sat_per_kwu(625); // 2.5 sat/vb
    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_rate(feerate);
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);

    assert_eq!(sent, original_sent_received.0);
    assert_eq!(received + fee, original_sent_received.1 + original_fee);
    assert!(fee > original_fee);

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.output.len(), 2);
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey == addr.script_pubkey())
            .unwrap()
            .value,
        Amount::from_sat(25_000)
    );
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey != addr.script_pubkey())
            .unwrap()
            .value,
        received
    );

    assert_fee_rate!(psbt, fee, feerate, @add_signature);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    // The original paid 142 sats, the replacement must add 1 sat/vB for its own ~142 vB.
    builder.fee_absolute(Amount::from_sat(300));
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);

    assert_eq!(sent, original_sent_received.0);
    assert_eq!(received + fee, original_sent_received.1 + original_fee);
    assert!(fee > original_fee, "{fee} > {original_fee}");

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.output.len(), 2);
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey == addr.script_pubkey())
            .unwrap()
            .value,
        Amount::from_sat(25_000)
    );
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey != addr.script_pubkey())
            .unwrap()
            .value,
        received
    );

    assert_eq!(fee, Amount::from_sat(300));
}

#[test]
fn test_bump_fee_reduce_single_recipient() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx();
    builder.drain_to(addr.script_pubkey()).drain_wallet();
    let psbt = builder.finish().unwrap();
    let tx = psbt.clone().extract_tx().expect("failed to extract tx");
    let original_sent_received = wallet.sent_and_received(&tx);
    let original_fee = check_fee!(wallet, psbt);
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let feerate = FeeRate::from_sat_per_kwu(625); // 2.5 sat/vb
    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .fee_rate(feerate)
        // remove original tx drain_to address and amount
        .set_recipients(Vec::new())
        // set back original drain_to address
        .drain_to(addr.script_pubkey())
        // drain wallet output amount will be re-calculated with new fee rate
        .drain_wallet();
    let psbt = builder.finish().unwrap();
    let (sent, _received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);

    assert_eq!(sent, original_sent_received.0);
    assert!(fee > original_fee);

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.output.len(), 1);
    assert_eq!(tx.output[0].value + fee, sent);

    assert_fee_rate!(psbt, fee, feerate, @add_signature);
}

#[test]
fn test_bump_fee_absolute_reduce_single_recipient() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx();
    builder.drain_to(addr.script_pubkey()).drain_wallet();
    let psbt = builder.finish().unwrap();
    let original_fee = check_fee!(wallet, psbt);
    let tx = psbt.extract_tx().expect("failed to extract tx");
    let original_sent_received = wallet.sent_and_received(&tx);
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .fee_absolute(Amount::from_sat(300))
        // remove original tx drain_to address and amount
        .set_recipients(Vec::new())
        // set back original drain_to address
        .drain_to(addr.script_pubkey())
        // drain wallet output amount will be re-calculated with new fee rate
        .drain_wallet();
    let psbt = builder.finish().unwrap();
    let tx = &psbt.unsigned_tx;
    let (sent, _received) = wallet.sent_and_received(tx);
    let fee = check_fee!(wallet, psbt);

    assert_eq!(sent, original_sent_received.0);
    assert!(fee > original_fee);

    assert_eq!(tx.output.len(), 1);
    assert_eq!(tx.output[0].value + fee, sent);

    assert_eq!(fee, Amount::from_sat(300));
}

#[test]
fn test_bump_fee_drain_wallet() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    // receive an extra tx so that our wallet has two utxos.
    let tx = Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![TxOut {
            script_pubkey: wallet
                .next_unused_address(KeychainKind::External)
                .script_pubkey(),
            value: Amount::from_sat(25_000),
        }],
    };
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx.clone());
    let anchor = ConfirmationBlockTime {
        block_id: wallet.latest_checkpoint().block_id(),
        confirmation_time: 42_000,
    };
    insert_anchor(&mut wallet, txid, anchor);

    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();

    let mut builder = wallet.build_tx();
    builder
        .drain_to(addr.script_pubkey())
        .add_utxo(OutPoint {
            txid: tx.compute_txid(),
            vout: 0,
        })
        .unwrap()
        .manually_selected_only();
    let psbt = builder.finish().unwrap();
    let tx = psbt.extract_tx().expect("failed to extract tx");
    let original_sent_received = wallet.sent_and_received(&tx);

    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);
    assert_eq!(original_sent_received.0, Amount::from_sat(25_000));

    // for the new feerate, it should be enough to reduce the output, but since we specify
    // `drain_wallet` we expect to spend everything
    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .drain_wallet()
        .fee_rate(FeeRate::from_sat_per_vb_u32(5));
    let psbt = builder.finish().unwrap();
    let (sent, _received) =
        wallet.sent_and_received(&psbt.extract_tx().expect("failed to extract tx"));

    assert_eq!(sent, Amount::from_sat(75_000));
}

#[test]
#[should_panic(expected = "InsufficientFunds")]
fn test_bump_fee_remove_output_manually_selected_only() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    // receive an extra tx so that our wallet has two utxos. then we manually pick only one of
    // them, and make sure that `bump_fee` doesn't try to add more. This fails because we've
    // told the wallet it's not allowed to add more inputs AND it can't reduce the value of the
    // existing output. In other words, bump_fee + manually_selected_only is always an error
    // unless there is a change output.
    let init_tx = Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![TxOut {
            script_pubkey: wallet
                .next_unused_address(KeychainKind::External)
                .script_pubkey(),
            value: Amount::from_sat(25_000),
        }],
    };

    let position: ChainPosition<ConfirmationBlockTime> =
        wallet.transactions().last().unwrap().chain_position;
    insert_tx(&mut wallet, init_tx.clone());
    match position {
        ChainPosition::Confirmed { anchor, .. } => {
            insert_anchor(&mut wallet, init_tx.compute_txid(), anchor)
        }
        other => panic!("all wallet txs must be confirmed: {other:?}"),
    }

    let outpoint = OutPoint {
        txid: init_tx.compute_txid(),
        vout: 0,
    };
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx();
    builder
        .drain_to(addr.script_pubkey())
        .add_utxo(outpoint)
        .unwrap()
        .manually_selected_only();
    let psbt = builder.finish().unwrap();
    let tx = psbt.extract_tx().expect("failed to extract tx");
    let original_sent_received = wallet.sent_and_received(&tx);
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);
    assert_eq!(original_sent_received.0, Amount::from_sat(25_000));

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .manually_selected_only()
        .fee_rate(FeeRate::from_sat_per_vb_u32(255));
    builder.finish().unwrap();
}

#[test]
fn test_bump_fee_add_input() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let init_tx = Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![TxOut {
            script_pubkey: wallet
                .next_unused_address(KeychainKind::External)
                .script_pubkey(),
            value: Amount::from_sat(25_000),
        }],
    };
    let txid = init_tx.compute_txid();
    let pos: ChainPosition<ConfirmationBlockTime> =
        wallet.transactions().last().unwrap().chain_position;
    insert_tx(&mut wallet, init_tx);
    match pos {
        ChainPosition::Confirmed { anchor, .. } => insert_anchor(&mut wallet, txid, anchor),
        other => panic!("all wallet txs must be confirmed: {other:?}"),
    }

    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx().coin_selection(LargestFirstCoinSelection);
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(45_000));
    let psbt = builder.finish().unwrap();
    let tx = psbt.extract_tx().expect("failed to extract tx");
    let original_details = wallet.sent_and_received(&tx);
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_rate(FeeRate::from_sat_per_vb_u32(50));
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);
    assert_eq!(sent, original_details.0 + Amount::from_sat(25_000));
    assert_eq!(fee + received, Amount::from_sat(30_000));

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.input.len(), 2);
    assert_eq!(tx.output.len(), 2);
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey == addr.script_pubkey())
            .unwrap()
            .value,
        Amount::from_sat(45_000)
    );
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey != addr.script_pubkey())
            .unwrap()
            .value,
        received
    );

    assert_fee_rate!(psbt, fee, FeeRate::from_sat_per_vb_u32(50), @add_signature);
}

#[test]
fn test_bump_fee_absolute_add_input() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    receive_output_in_latest_block(&mut wallet, Amount::from_sat(25_000));
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx().coin_selection(LargestFirstCoinSelection);
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(45_000));
    let psbt = builder.finish().unwrap();
    let tx = psbt.extract_tx().expect("failed to extract tx");
    let original_sent_received = wallet.sent_and_received(&tx);
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_absolute(Amount::from_sat(6_000));
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);

    assert_eq!(sent, original_sent_received.0 + Amount::from_sat(25_000));
    assert_eq!(fee + received, Amount::from_sat(30_000));

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.input.len(), 2);
    assert_eq!(tx.output.len(), 2);
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey == addr.script_pubkey())
            .unwrap()
            .value,
        Amount::from_sat(45_000)
    );
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey != addr.script_pubkey())
            .unwrap()
            .value,
        received
    );

    assert_eq!(fee, Amount::from_sat(6_000));
}

#[test]
fn test_bump_fee_no_change_add_input_and_change() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let op = receive_output_in_latest_block(&mut wallet, Amount::from_sat(25_000));

    // initially make a tx without change by using `drain_to`
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx();
    builder
        .drain_to(addr.script_pubkey())
        .add_utxo(op)
        .unwrap()
        .manually_selected_only();
    let psbt = builder.finish().unwrap();
    let original_sent_received =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let original_fee = check_fee!(wallet, psbt);

    let tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    // Now bump the fees, the wallet should add an extra input and a change output, and leave
    // the original output untouched.
    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_rate(FeeRate::from_sat_per_vb_u32(50));
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);

    let original_send_all_amount = original_sent_received.0 - original_fee;
    assert_eq!(sent, original_sent_received.0 + Amount::from_sat(50_000));
    assert_eq!(
        received,
        Amount::from_sat(75_000) - original_send_all_amount - fee
    );

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.input.len(), 2);
    assert_eq!(tx.output.len(), 2);
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey == addr.script_pubkey())
            .unwrap()
            .value,
        original_send_all_amount
    );
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey != addr.script_pubkey())
            .unwrap()
            .value,
        Amount::from_sat(75_000) - original_send_all_amount - fee
    );

    assert_fee_rate!(psbt, fee, FeeRate::from_sat_per_vb_u32(50), @add_signature);
}

#[test]
fn test_bump_fee_force_add_input() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let incoming_op = receive_output_in_latest_block(&mut wallet, Amount::from_sat(25_000));

    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx().coin_selection(LargestFirstCoinSelection);
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(45_000));
    let psbt = builder.finish().unwrap();
    let mut tx = psbt.extract_tx().expect("failed to extract tx");
    let original_sent_received = wallet.sent_and_received(&tx);
    let txid = tx.compute_txid();
    for txin in &mut tx.input {
        txin.witness.push([0x00; P2WPKH_FAKE_SIG_SIZE]); // sig (72)
        txin.witness.push([0x00; P2WPKH_FAKE_PK_SIZE]); // pk (33)
    }
    insert_tx(&mut wallet, tx.clone());
    // the new fee_rate is low enough that just reducing the change would be fine, but we force
    // the addition of an extra input with `add_utxo()`
    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .add_utxo(incoming_op)
        .unwrap()
        .fee_rate(FeeRate::from_sat_per_vb_u32(5));
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);

    assert_eq!(sent, original_sent_received.0 + Amount::from_sat(25_000));
    assert_eq!(fee + received, Amount::from_sat(30_000));

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.input.len(), 2);
    assert_eq!(tx.output.len(), 2);
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey == addr.script_pubkey())
            .unwrap()
            .value,
        Amount::from_sat(45_000)
    );
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey != addr.script_pubkey())
            .unwrap()
            .value,
        received
    );

    assert_fee_rate!(psbt, fee, FeeRate::from_sat_per_vb_u32(5), @add_signature);
}

#[test]
fn test_bump_fee_absolute_force_add_input() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let incoming_op = receive_output_in_latest_block(&mut wallet, Amount::from_sat(25_000));

    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx().coin_selection(LargestFirstCoinSelection);
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(45_000));
    let psbt = builder.finish().unwrap();
    let mut tx = psbt.extract_tx().expect("failed to extract tx");
    let original_sent_received = wallet.sent_and_received(&tx);
    let txid = tx.compute_txid();
    // skip saving the new utxos, we know they can't be used anyways
    for txin in &mut tx.input {
        txin.witness.push([0x00; P2WPKH_FAKE_SIG_SIZE]); // sig (72)
        txin.witness.push([0x00; P2WPKH_FAKE_PK_SIZE]); // pk (33)
    }
    insert_tx(&mut wallet, tx.clone());

    // the new fee_rate is low enough that just reducing the change would be fine, but we force
    // the addition of an extra input with `add_utxo()`
    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .add_utxo(incoming_op)
        .unwrap()
        // The original paid 142 sats, the replacement must add 1 sat/vB for its own ~210 vB.
        .fee_absolute(Amount::from_sat(400));
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);

    assert_eq!(sent, original_sent_received.0 + Amount::from_sat(25_000));
    assert_eq!(fee + received, Amount::from_sat(30_000));

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.input.len(), 2);
    assert_eq!(tx.output.len(), 2);
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey == addr.script_pubkey())
            .unwrap()
            .value,
        Amount::from_sat(45_000)
    );
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey != addr.script_pubkey())
            .unwrap()
            .value,
        received
    );

    assert_eq!(fee, Amount::from_sat(400));
}

#[test]
#[should_panic(expected = "InsufficientFunds")]
fn test_bump_fee_unconfirmed_inputs_only() {
    // We try to bump the fee, but:
    // - We can't reduce the change, as we have no change
    // - All our UTXOs are unconfirmed
    // So, we fail with "InsufficientFunds", as per RBF rule 2:
    // The replacement transaction may only include an unconfirmed input
    // if that input was included in one of the original transactions.
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx();
    builder.drain_wallet().drain_to(addr.script_pubkey());
    let psbt = builder.finish().unwrap();
    // Now we receive one transaction with 0 confirmations. We won't be able to use that for
    // fee bumping, as it's still unconfirmed!
    receive_output(&mut wallet, Amount::from_sat(25_000), ReceiveTo::Mempool(0));
    let mut tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    for txin in &mut tx.input {
        txin.witness.push([0x00; P2WPKH_FAKE_SIG_SIZE]); // sig (72)
        txin.witness.push([0x00; P2WPKH_FAKE_PK_SIZE]); // pk (33)
    }
    insert_tx(&mut wallet, tx);
    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_rate(FeeRate::from_sat_per_vb_u32(25));
    builder.finish().unwrap();
}

#[test]
fn test_bump_fee_unconfirmed_input() {
    // We create a tx draining the wallet and spending one confirmed
    // and one unconfirmed UTXO. We check that we can fee bump normally
    // (BIP125 rule 2 only apply to newly added unconfirmed input, you can
    // always fee bump with an unconfirmed input if it was included in the
    // original transaction)
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    // We receive a tx with 0 confirmations, which will be used as an input
    // in the drain tx.
    receive_output(&mut wallet, Amount::from_sat(25_000), ReceiveTo::Mempool(0));
    let mut builder = wallet.build_tx();
    builder.drain_wallet().drain_to(addr.script_pubkey());
    let psbt = builder.finish().unwrap();
    let mut tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    for txin in &mut tx.input {
        txin.witness.push([0x00; P2WPKH_FAKE_SIG_SIZE]); // sig (72)
        txin.witness.push([0x00; P2WPKH_FAKE_PK_SIZE]); // pk (33)
    }
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .fee_rate(FeeRate::from_sat_per_vb_u32(15))
        // remove original tx drain_to address and amount
        .set_recipients(Vec::new())
        // set back original drain_to address
        .drain_to(addr.script_pubkey())
        // drain wallet output amount will be re-calculated with new fee rate
        .drain_wallet();
    builder.finish().unwrap();
}

#[test]
#[should_panic(expected = "FeeTooLow")]
fn test_legacy_bump_fee_zero_abs() {
    let (mut wallet, _) = get_funded_wallet_single(get_test_pkh());
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut builder = wallet.build_tx();
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(25_000));
    let psbt = builder.finish().unwrap();

    let tx = psbt.extract_tx().expect("failed to extract tx");
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_absolute(Amount::ZERO);
    builder.finish().unwrap();
}

#[test]
fn test_legacy_bump_fee_drain_wallet() {
    let (mut wallet, _) = get_funded_wallet_single(get_test_pkh());
    // receive an extra tx so that our wallet has two utxos.
    let tx = Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![TxOut {
            value: Amount::from_sat(25_000),
            script_pubkey: wallet
                .next_unused_address(KeychainKind::External)
                .script_pubkey(),
        }],
    };
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx.clone());
    let anchor = ConfirmationBlockTime {
        block_id: wallet.latest_checkpoint().block_id(),
        confirmation_time: 42_000,
    };
    insert_anchor(&mut wallet, txid, anchor);

    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();

    let mut builder = wallet.build_tx();
    builder
        .drain_to(addr.script_pubkey())
        .add_utxo(OutPoint {
            txid: tx.compute_txid(),
            vout: 0,
        })
        .unwrap()
        .manually_selected_only();
    let psbt = builder.finish().unwrap();
    let tx = psbt.extract_tx().expect("failed to extract tx");
    let original_sent_received = wallet.sent_and_received(&tx);

    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);
    assert_eq!(original_sent_received.0, Amount::from_sat(25_000));

    // for the new feerate, it should be enough to reduce the output, but since we specify
    // `drain_wallet` we expect to spend everything
    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder
        .drain_wallet()
        .fee_rate(FeeRate::from_sat_per_vb_u32(5));
    let psbt = builder.finish().unwrap();
    let (sent, _received) =
        wallet.sent_and_received(&psbt.extract_tx().expect("failed to extract tx"));

    assert_eq!(sent, Amount::from_sat(75_000));
}

#[test]
fn test_legacy_bump_fee_add_input() {
    let (mut wallet, _) = get_funded_wallet_single(get_test_pkh());
    let init_tx = Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![TxOut {
            script_pubkey: wallet
                .next_unused_address(KeychainKind::External)
                .script_pubkey(),
            value: Amount::from_sat(25_000),
        }],
    };
    let txid = init_tx.compute_txid();
    let pos: ChainPosition<ConfirmationBlockTime> =
        wallet.transactions().last().unwrap().chain_position;
    insert_tx(&mut wallet, init_tx);
    match pos {
        ChainPosition::Confirmed { anchor, .. } => insert_anchor(&mut wallet, txid, anchor),
        other => panic!("all wallet txs must be confirmed: {other:?}"),
    }

    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx().coin_selection(LargestFirstCoinSelection);
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(45_000));
    let psbt = builder.finish().unwrap();
    let tx = psbt.extract_tx().expect("failed to extract tx");
    let original_details = wallet.sent_and_received(&tx);
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_rate(FeeRate::from_sat_per_vb_u32(50));
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);
    assert_eq!(sent, original_details.0 + Amount::from_sat(25_000));
    assert_eq!(fee + received, Amount::from_sat(30_000));

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.input.len(), 2);
    assert_eq!(tx.output.len(), 2);
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey == addr.script_pubkey())
            .unwrap()
            .value,
        Amount::from_sat(45_000)
    );
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey != addr.script_pubkey())
            .unwrap()
            .value,
        received
    );

    assert_fee_rate_legacy!(psbt, fee, FeeRate::from_sat_per_vb_u32(50), @add_signature);
}

#[test]
fn test_legacy_bump_fee_absolute_add_input() {
    let (mut wallet, _) = get_funded_wallet_single(get_test_pkh());
    receive_output_in_latest_block(&mut wallet, Amount::from_sat(25_000));
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let mut builder = wallet.build_tx().coin_selection(LargestFirstCoinSelection);
    builder.add_recipient(addr.script_pubkey(), Amount::from_sat(45_000));
    let psbt = builder.finish().unwrap();
    let tx = psbt.extract_tx().expect("failed to extract tx");
    let (original_sent, _original_received) = wallet.sent_and_received(&tx);
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    let mut builder = wallet.build_fee_bump(txid).unwrap();
    builder.fee_absolute(Amount::from_sat(6_000));
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet, psbt);

    assert_eq!(sent, original_sent + Amount::from_sat(25_000));
    assert_eq!(fee + received, Amount::from_sat(30_000));

    let tx = &psbt.unsigned_tx;
    assert_eq!(tx.input.len(), 2);
    assert_eq!(tx.output.len(), 2);
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey == addr.script_pubkey())
            .unwrap()
            .value,
        Amount::from_sat(45_000)
    );
    assert_eq!(
        tx.output
            .iter()
            .find(|txout| txout.script_pubkey != addr.script_pubkey())
            .unwrap()
            .value,
        received
    );

    assert_eq!(fee, Amount::from_sat(6_000));
}

// Test that we can fee-bump a tx containing a foreign (p2a) utxo.
#[test]
fn test_bump_fee_pay_to_anchor_foreign_utxo() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let drain_spk = wallet
        .next_unused_address(KeychainKind::External)
        .script_pubkey();

    let witness_utxo = TxOut {
        value: Amount::ONE_SAT,
        script_pubkey: bitcoin::ScriptBuf::new_p2a(),
    };
    // Remember to include this as a "floating" txout in the wallet.
    let outpoint = OutPoint::new(Hash::hash(b"prev"), 1);
    wallet.insert_txout(outpoint, witness_utxo.clone());
    let satisfaction_weight = Weight::from_wu(71);
    let psbt_input = psbt::Input {
        witness_utxo: Some(witness_utxo),
        ..Default::default()
    };

    let mut tx_builder = wallet.build_tx();
    tx_builder
        .add_foreign_utxo(outpoint, psbt_input, satisfaction_weight)
        .unwrap()
        .only_witness_utxo()
        .fee_rate(FeeRate::from_sat_per_vb_u32(2))
        .drain_to(drain_spk.clone());
    let psbt = tx_builder.finish().unwrap();
    let tx = psbt.unsigned_tx.clone();
    assert!(tx.input.iter().any(|txin| txin.previous_output == outpoint));
    let txid1 = tx.compute_txid();
    wallet.apply_unconfirmed_txs([(tx, 123456)]);

    // Now build fee bump.
    let mut tx_builder = wallet
        .build_fee_bump(txid1)
        .expect("`build_fee_bump` should succeed");
    tx_builder
        .set_recipients(vec![])
        .drain_to(drain_spk)
        .only_witness_utxo()
        .fee_rate(FeeRate::from_sat_per_vb_u32(5));
    let psbt = tx_builder.finish().unwrap();
    let tx = &psbt.unsigned_tx;
    assert!(tx.input.iter().any(|txin| txin.previous_output == outpoint));
}

use assert_matches::assert_matches;
use bdk_wallet::KeychainKind;
use bdk_wallet::error::CreateTxError;
use bdk_wallet::test_utils::*;
use bitcoin::{Amount, FeeRate};

mod common;
use common::*;

/// Sending to an address owned by the wallet returns `PaymentToOwnAddress` by default.
#[test]
fn test_payment_to_own_address_is_rejected() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let own_addr = wallet.next_unused_address(KeychainKind::External);

    let mut builder = wallet.build_tx();
    builder.add_recipient(own_addr.script_pubkey(), Amount::from_sat(25_000));
    let result = builder.finish();

    assert_matches!(
        result,
        Err(CreateTxError::PaymentToOwnAddress { index: 0 }),
        "expected PaymentToOwnAddress error when sending to own address without allow_self_payment"
    );
}

/// With `.allow_self_payment()` the transaction is built successfully.
#[test]
fn test_payment_to_own_address_allowed_with_flag() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let own_addr = wallet.next_unused_address(KeychainKind::External);

    let mut builder = wallet.build_tx();
    builder
        .add_recipient(own_addr.script_pubkey(), Amount::from_sat(25_000))
        .allow_self_payment();
    let result = builder.finish();

    assert!(
        result.is_ok(),
        "expected Ok when allow_self_payment is set, got: {:?}",
        result.err()
    );
}

/// After building a self-payment transaction the output index is stored in
/// the wallet's staged changeset under `self_payment_recipients`.
#[test]
fn test_self_payment_tracked_in_changeset() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let own_addr = wallet.next_unused_address(KeychainKind::External);

    let mut builder = wallet.build_tx();
    builder
        .add_recipient(own_addr.script_pubkey(), Amount::from_sat(25_000))
        .allow_self_payment();
    let psbt = builder.finish().unwrap();
    let txid = psbt.unsigned_tx.compute_txid();

    // The staged changeset must contain the self-payment entry.
    let staged = wallet
        .staged()
        .expect("staged changeset should be non-empty");
    assert!(
        staged.self_payment_recipients.contains_key(&txid),
        "expected txid {txid} in self_payment_recipients"
    );

    // The entry must be Some (not a removal marker).
    let tracked_indices = staged.self_payment_recipients[&txid]
        .as_ref()
        .expect("expected Some(indices), not None");
    assert!(
        !tracked_indices.is_empty(),
        "expected at least one tracked output index"
    );

    // Every tracked index must correspond to an output owned by the wallet.
    for &idx in tracked_indices {
        let txout = &psbt.unsigned_tx.output[idx as usize];
        assert!(
            wallet.is_mine(txout.script_pubkey.clone()),
            "tracked output at index {idx} should be owned by the wallet"
        );
    }
}

/// Fee-bumping a transaction that contains a self-payment recipient preserves
/// that recipient — it must not be stripped as change by `build_fee_bump`.
#[test]
fn test_build_fee_bump_preserves_self_payment_recipient() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let own_addr = wallet.next_unused_address(KeychainKind::External);

    // Build a transaction that sends to our own address.
    let mut builder = wallet.build_tx();
    builder
        .add_recipient(own_addr.script_pubkey(), Amount::from_sat(25_000))
        .fee_rate(FeeRate::from_sat_per_vb_u32(10))
        .allow_self_payment();
    let psbt = builder.finish().unwrap();
    let mut tx = psbt.extract_tx().unwrap();

    // Add fake witness data so tx.weight() reflects signed weight, keeping
    // the actual fee rate close to the requested 10 sat/vB.
    for txin in &mut tx.input {
        txin.witness.push([0x00; P2WPKH_FAKE_SIG_SIZE]);
        txin.witness.push([0x00; P2WPKH_FAKE_PK_SIZE]);
    }
    let txid = tx.compute_txid();
    insert_tx(&mut wallet, tx);

    // Fee-bump the transaction at a higher rate.
    let mut bump_builder = wallet.build_fee_bump(txid).unwrap();
    bump_builder
        .fee_rate(FeeRate::from_sat_per_vb_u32(20))
        .allow_self_payment();
    let bump_psbt = bump_builder.finish().expect("fee bump should succeed");
    let bump_tx = bump_psbt.unsigned_tx;

    // The replacement must still contain the self-payment recipient output.
    let self_payment_present = bump_tx
        .output
        .iter()
        .any(|o| o.script_pubkey == own_addr.script_pubkey());

    assert!(
        self_payment_present,
        "self-payment recipient must be preserved in the fee-bumped transaction"
    );
}

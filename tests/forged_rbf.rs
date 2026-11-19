//! Tests for the forged-replacement attack: a malicious chain source hides a transaction the wallet
//! broadcast and returns a conflicting transaction with invalid signatures that pays an attacker,
//! hoping the user fee-bumps it.

use std::str::FromStr;
use std::sync::Arc;

use assert_matches::assert_matches;
use bdk_chain::{BlockId, ConfirmationBlockTime, TxUpdate};
use bdk_wallet::error::BuildFeeBumpError;
use bdk_wallet::test_utils::*;
use bdk_wallet::{KeychainKind, TxRejectedReason, Update, Wallet, WalletEvent};
use bitcoin::{
    Address, Amount, BlockHash, OutPoint, ScriptBuf, TapSighashType, Transaction, TxOut, Txid,
    Weight, hashes::Hash, psbt,
};

mod common;
use common::*;

fn attacker_spk() -> ScriptBuf {
    Address::from_str("tb1q6yn66vajcctph75pvylgkksgpp6nq04ppwct9a")
        .unwrap()
        .assume_checked()
        .script_pubkey()
}

/// A legitimate third-party recipient.
fn recipient_spk() -> ScriptBuf {
    Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked()
        .script_pubkey()
}

/// Builds and signs a tx from `wallet` paying 10k sats to a third party.
fn signed_payment(wallet: &mut Wallet, descriptors: &[&str]) -> Transaction {
    let mut builder = wallet.build_tx();
    builder.add_recipient(recipient_spk(), Amount::from_sat(10_000));
    let psbt = builder.finish().unwrap();
    sign_and_extract_with(wallet, psbt, descriptors)
}

/// Forges a conflict of `tx`: same inputs and witnesses, but every output pays the attacker
/// and the fee is zero. The signatures commit to the original outputs so they are now invalid.
fn forge(wallet: &Wallet, tx: &Transaction) -> Transaction {
    let mut forged = tx.clone();
    let fee = wallet.calculate_fee(tx).unwrap();
    for txout in &mut forged.output {
        txout.script_pubkey = attacker_spk();
    }
    forged.output[0].value += fee;
    forged
}

fn rejections(events: &[WalletEvent]) -> Vec<(Txid, TxRejectedReason)> {
    events
        .iter()
        .filter_map(|event| match event {
            WalletEvent::TxRejected { txid, reason, .. } => Some((*txid, *reason)),
            _ => None,
        })
        .collect()
}

fn rejected_txids(events: &[WalletEvent]) -> Vec<Txid> {
    rejections(events)
        .into_iter()
        .map(|(txid, _)| txid)
        .collect()
}

const INVALID_FIRST_INPUT: TxRejectedReason = TxRejectedReason::InvalidInput { input_index: 0 };

fn is_canonical(wallet: &Wallet, txid: Txid) -> bool {
    wallet.transactions().any(|tx| tx.tx_node.txid == txid)
}

#[test]
fn forged_conflict_is_rejected_by_apply_update() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _) = get_funded_wallet(desc, change_desc);
    let tx = signed_payment(&mut wallet, &[desc, change_desc]);
    let txid = tx.compute_txid();
    wallet.apply_unconfirmed_txs([(tx.clone(), 100)]);
    assert!(is_canonical(&wallet, txid));

    // The malicious server hides `tx` (evicts it) and returns a forged conflict instead.
    let forged = forge(&wallet, &tx);
    let forged_txid = forged.compute_txid();
    let mut tx_update = TxUpdate::default();
    tx_update.txs = vec![Arc::new(forged)];
    tx_update.seen_ats = [(forged_txid, 200)].into();
    tx_update.evicted_ats = [(txid, 200)].into();
    let events = wallet
        .apply_update_events(Update {
            tx_update,
            ..Default::default()
        })
        .unwrap();

    assert_eq!(
        rejections(&events),
        vec![(forged_txid, INVALID_FIRST_INPUT)]
    );
    assert_eq!(events.len(), 1, "no other event expected: {events:?}");
    assert!(wallet.tx_graph().get_tx(forged_txid).is_none());
    // The eviction relied on the forged conflict, so it is dropped too.
    assert!(is_canonical(&wallet, txid));

    assert_matches!(
        wallet.build_fee_bump(forged_txid),
        Err(BuildFeeBumpError::TransactionNotFound(_))
    );
    assert!(wallet.build_fee_bump(txid).is_ok());
}

#[test]
fn forged_conflict_is_rejected_by_apply_unconfirmed_txs() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _) = get_funded_wallet(desc, change_desc);
    let tx = signed_payment(&mut wallet, &[desc, change_desc]);
    let txid = tx.compute_txid();
    wallet.apply_unconfirmed_txs([(tx.clone(), 100)]);

    let forged = forge(&wallet, &tx);
    let forged_txid = forged.compute_txid();
    let events = wallet.apply_unconfirmed_txs_events([(forged, 200)]);

    assert_eq!(
        rejections(&events),
        vec![(forged_txid, INVALID_FIRST_INPUT)]
    );
    assert!(wallet.tx_graph().get_tx(forged_txid).is_none());
    assert!(is_canonical(&wallet, txid));
}

#[test]
fn fee_bump_refuses_forged_tx_already_in_wallet() {
    // A forged tx may already be persisted by a wallet that synced before the fix.
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _) = get_funded_wallet(desc, change_desc);
    let tx = signed_payment(&mut wallet, &[desc, change_desc]);
    let forged = forge(&wallet, &tx);
    let forged_txid = forged.compute_txid();

    let mut changeset = wallet.staged().unwrap().clone();
    changeset.tx_graph.txs.insert(Arc::new(forged));
    changeset.tx_graph.last_seen.insert(forged_txid, 200);
    let mut wallet = Wallet::load()
        .load_wallet_no_persist(changeset)
        .unwrap()
        .unwrap();
    assert!(is_canonical(&wallet, forged_txid));

    assert_matches!(
        wallet.build_fee_bump(forged_txid),
        Err(BuildFeeBumpError::IrreplaceableTransaction(t)) if t == forged_txid
    );
}

/// A validly signed taproot spend with a foreign input whose prevout is unknown can't be bumped
/// because its fee is unknown, and is reported as such (not as a forged tx). Once the prevout is
/// known, the signatures verify and the tx can be bumped.
#[test]
fn fee_bump_of_taproot_spend_with_unknown_foreign_prevout() {
    let desc = get_test_tr_single_sig();
    let (mut wallet, _) = get_funded_wallet_single(desc);

    let foreign_op = OutPoint::new(Hash::hash(b"prev"), 1);
    let foreign_txout = TxOut {
        value: Amount::from_sat(1_000),
        script_pubkey: ScriptBuf::new_p2a(),
    };
    let mut builder = wallet.build_tx();
    builder
        .add_foreign_utxo(
            foreign_op,
            psbt::Input {
                witness_utxo: Some(foreign_txout.clone()),
                ..Default::default()
            },
            Weight::from_wu(71),
        )
        .unwrap()
        .only_witness_utxo()
        .add_recipient(recipient_spk(), Amount::from_sat(10_000));
    let psbt = builder.finish().unwrap();
    let tx = sign_and_extract_with(&wallet, psbt, &[desc]);
    let txid = tx.compute_txid();

    // Sync-time verification rejects it, so insert it directly, as an older version would have.
    let mut changeset = wallet.staged().unwrap().clone();
    changeset.tx_graph.txs.insert(Arc::new(tx));
    changeset.tx_graph.last_seen.insert(txid, 200);
    let mut wallet = Wallet::load()
        .load_wallet_no_persist(changeset)
        .unwrap()
        .unwrap();
    assert!(is_canonical(&wallet, txid));

    assert_matches!(
        wallet.build_fee_bump(txid),
        Err(BuildFeeBumpError::FeeRateUnavailable)
    );

    wallet.insert_txout(foreign_op, foreign_txout);
    assert!(wallet.build_fee_bump(txid).is_ok());
}

/// A txid doesn't commit to witnesses: if the graph holds an unsigned copy of a tx, a signed copy
/// with the same txid in an update must be accepted (not rejected because of the graph's copy), so
/// that the graph picks up its witnesses.
#[test]
fn signed_copy_in_update_replaces_unsigned_copy_in_graph() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _) = get_funded_wallet(desc, change_desc);
    let tx = signed_payment(&mut wallet, &[desc, change_desc]);
    let txid = tx.compute_txid();
    let mut unsigned = tx.clone();
    for txin in &mut unsigned.input {
        txin.witness.clear();
    }
    assert_eq!(unsigned.compute_txid(), txid);

    // The unsigned copy may be persisted by a wallet that synced before the fix.
    let mut changeset = wallet.staged().unwrap().clone();
    changeset.tx_graph.txs.insert(Arc::new(unsigned));
    changeset.tx_graph.last_seen.insert(txid, 100);
    let mut wallet = Wallet::load()
        .load_wallet_no_persist(changeset)
        .unwrap()
        .unwrap();
    // The graph holds the unsigned copy, so fee-bumping is refused.
    assert_matches!(
        wallet.build_fee_bump(txid),
        Err(BuildFeeBumpError::IrreplaceableTransaction(_))
    );

    let mut tx_update = TxUpdate::default();
    tx_update.txs = vec![Arc::new(tx.clone())];
    tx_update.seen_ats = [(txid, 200)].into();
    let events = wallet
        .apply_update_events(Update {
            tx_update,
            ..Default::default()
        })
        .unwrap();

    assert!(rejected_txids(&events).is_empty(), "{events:?}");
    assert_eq!(*wallet.tx_graph().get_tx(txid).unwrap(), tx);
    assert!(wallet.build_fee_bump(txid).is_ok());
}

#[test]
fn received_payment_is_not_verified() {
    // Only wallet-owned inputs are verified: an incoming payment with a foreign input whose
    // prevout is unknown is accepted, whatever its witness.
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _) = get_funded_wallet(desc, change_desc);
    let mut tx = new_tx(0);
    tx.input.push(bitcoin::TxIn {
        previous_output: OutPoint::new(Hash::hash(b"foreign"), 0),
        ..Default::default()
    });
    tx.output.push(TxOut {
        value: Amount::from_sat(10_000),
        script_pubkey: wallet
            .next_unused_address(KeychainKind::External)
            .script_pubkey(),
    });
    let events = wallet.apply_unconfirmed_txs_events([(tx.clone(), 100)]);
    assert!(rejected_txids(&events).is_empty());
    assert!(is_canonical(&wallet, tx.compute_txid()));
}

/// Signed txs are accepted, while forged or unsigned spends are rejected, for every supported
/// script type (including taproot key and script path spends, and timelocked scripts).
#[test]
fn verification_covers_descriptor_types() {
    for desc in [
        get_test_pkh(),
        get_test_wpkh(),
        get_test_single_sig_csv(),
        get_test_single_sig_cltv(),
        get_test_tr_single_sig(),
        get_test_tr_single_sig_xprv(),
        // Internal key is public only, so this is a script path spend.
        get_test_tr_with_taptree(),
    ] {
        let (mut wallet, _) = get_funded_wallet_single(desc);
        let tx = signed_payment(&mut wallet, &[desc]);
        let forged = forge(&wallet, &tx);
        let mut unsigned = tx.clone();
        for txin in &mut unsigned.input {
            txin.script_sig = ScriptBuf::new();
            txin.witness.clear();
        }

        for bad in [forged, unsigned] {
            let events = wallet.apply_unconfirmed_txs_events([(bad.clone(), 100)]);
            assert_eq!(
                rejections(&events),
                vec![(bad.compute_txid(), INVALID_FIRST_INPUT)],
                "{desc}: invalid spend should be rejected"
            );
        }
        let events = wallet.apply_unconfirmed_txs_events([(tx.clone(), 100)]);
        assert!(
            rejected_txids(&events).is_empty(),
            "{desc}: valid spend should be accepted"
        );
        assert!(is_canonical(&wallet, tx.compute_txid()), "{desc}");
    }
}

/// Taproot signatures (other than `SIGHASH_ANYONECANPAY` ones) commit to all prevouts: a taproot
/// spend with a foreign input can only be verified, and accepted, once the foreign prevout is
/// known.
#[test]
fn taproot_spend_with_unknown_foreign_prevout() {
    let desc = get_test_tr_single_sig();
    let (mut wallet, _) = get_funded_wallet_single(desc);

    let foreign_op = OutPoint::new(Hash::hash(b"prev"), 1);
    let foreign_txout = TxOut {
        value: Amount::ONE_SAT,
        script_pubkey: ScriptBuf::new_p2a(),
    };
    let mut builder = wallet.build_tx();
    builder
        .add_foreign_utxo(
            foreign_op,
            psbt::Input {
                witness_utxo: Some(foreign_txout.clone()),
                ..Default::default()
            },
            Weight::from_wu(71),
        )
        .unwrap()
        .only_witness_utxo()
        .add_recipient(recipient_spk(), Amount::from_sat(10_000));
    let psbt = builder.finish().unwrap();
    let tx = sign_and_extract_with(&wallet, psbt, &[desc]);
    let txid = tx.compute_txid();

    // The foreign prevout is unknown, so the wallet's taproot signature can't be verified. The tx
    // is rejected rather than accepted unverified.
    let events = wallet.apply_unconfirmed_txs_events([(tx.clone(), 100)]);
    let [(rejected_txid, TxRejectedReason::MissingPrevouts { input_index })] =
        rejections(&events)[..]
    else {
        panic!("expected a single `MissingPrevouts` rejection: {events:?}");
    };
    assert_eq!(rejected_txid, txid);
    assert_ne!(
        tx.input[input_index].previous_output, foreign_op,
        "the reported input is the wallet's own"
    );

    // The chain source provides the foreign prevout in the update: the tx verifies.
    let mut tx_update = TxUpdate::default();
    tx_update.txs = vec![Arc::new(tx)];
    tx_update.txouts = [(foreign_op, foreign_txout)].into();
    tx_update.seen_ats = [(txid, 100)].into();
    let events = wallet
        .apply_update_events(Update {
            tx_update,
            ..Default::default()
        })
        .unwrap();
    assert!(rejected_txids(&events).is_empty(), "{events:?}");
    assert!(is_canonical(&wallet, txid));
}

/// `SIGHASH_ANYONECANPAY` taproot signatures only commit to the input's own prevout: a taproot
/// spend with a foreign input is verified, and accepted, even though the foreign prevout is
/// unknown.
#[test]
fn taproot_anyonecanpay_spend_with_unknown_foreign_prevout() {
    let desc = get_test_tr_single_sig();
    let (mut wallet, _) = get_funded_wallet_single(desc);

    let foreign_op = OutPoint::new(Hash::hash(b"prev"), 1);
    let mut builder = wallet.build_tx();
    builder
        .add_foreign_utxo(
            foreign_op,
            psbt::Input {
                witness_utxo: Some(TxOut {
                    value: Amount::ONE_SAT,
                    script_pubkey: ScriptBuf::new_p2a(),
                }),
                ..Default::default()
            },
            Weight::from_wu(71),
        )
        .unwrap()
        .only_witness_utxo()
        .add_recipient(recipient_spk(), Amount::from_sat(10_000));
    let mut psbt = builder.finish().unwrap();
    // Sign the wallet's input with `SIGHASH_ANYONECANPAY`, so that its signature only commits to
    // its own prevout. The foreign input is left unsigned.
    for (psbt_input, txin) in psbt.inputs.iter_mut().zip(&psbt.unsigned_tx.input) {
        if txin.previous_output != foreign_op {
            psbt_input.sighash_type = Some(TapSighashType::AllPlusAnyoneCanPay.into());
        }
    }
    let tx = sign_and_extract_with(&wallet, psbt, &[desc]);
    let txid = tx.compute_txid();

    let events = wallet.apply_unconfirmed_txs_events([(tx.clone(), 100)]);
    assert!(rejected_txids(&events).is_empty(), "{events:?}");
    assert!(is_canonical(&wallet, txid));

    // Tampering with the outputs still invalidates the signature. (`forge` can't be used: the fee
    // is unknown without the foreign prevout.)
    let mut forged = tx.clone();
    for txout in &mut forged.output {
        txout.script_pubkey = attacker_spk();
    }
    let forged_txid = forged.compute_txid();
    let events = wallet.apply_unconfirmed_txs_events([(forged, 200)]);
    assert_eq!(rejected_txids(&events), vec![forged_txid]);
}

/// Confirmed transactions are not verified when applied, and BDK doesn't validate proof of work:
/// a malicious chain source can forge a block confirming a forged transaction, then reorg it away.
/// The forged transaction then sits unverified and unconfirmed in the wallet, and only the check
/// in `build_fee_bump` prevents replacing it.
#[test]
fn forged_tx_confirmed_in_fake_block_then_reorged_is_not_bumpable() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _) = get_funded_wallet(desc, change_desc);
    let tx = signed_payment(&mut wallet, &[desc, change_desc]);
    let txid = tx.compute_txid();
    wallet.apply_unconfirmed_txs([(tx.clone(), 100)]);

    // The malicious server forges a block on top of the tip, confirming the forged conflict.
    let tip = wallet.latest_checkpoint();
    let fake_block = BlockId {
        height: tip.height() + 1,
        hash: BlockHash::hash(b"fake block"),
    };
    let forged = forge(&wallet, &tx);
    let forged_txid = forged.compute_txid();
    let mut tx_update = TxUpdate::default();
    tx_update.txs = vec![Arc::new(forged)];
    tx_update.anchors = [(
        ConfirmationBlockTime {
            block_id: fake_block,
            confirmation_time: 1_000,
        },
        forged_txid,
    )]
    .into();
    tx_update.seen_ats = [(forged_txid, 200)].into();
    let events = wallet
        .apply_update_events(Update {
            tx_update,
            chain: Some(tip.clone().push(fake_block).unwrap()),
            ..Default::default()
        })
        .unwrap();

    // Confirmed transactions are not verified: the forged tx is accepted and replaces `tx`.
    assert!(rejected_txids(&events).is_empty(), "{events:?}");
    assert!(is_canonical(&wallet, forged_txid));
    assert!(!is_canonical(&wallet, txid));
    assert_matches!(
        wallet.build_fee_bump(forged_txid),
        Err(BuildFeeBumpError::TransactionConfirmed(_))
    );

    // The server reorgs the fake block away. The forged tx is now unconfirmed and canonical
    // (it was seen last), without ever having been verified.
    let reorg_block = BlockId {
        height: fake_block.height,
        hash: BlockHash::hash(b"real block"),
    };
    wallet
        .apply_update(Update {
            chain: Some(tip.push(reorg_block).unwrap()),
            ..Default::default()
        })
        .unwrap();
    let forged_canonical = wallet
        .transactions()
        .find(|tx| tx.tx_node.txid == forged_txid)
        .expect("forged tx must be canonical");
    assert!(!forged_canonical.chain_position.is_confirmed());

    // The load-bearing check in `build_fee_bump` refuses to replace it.
    assert_matches!(
        wallet.build_fee_bump(forged_txid),
        Err(BuildFeeBumpError::IrreplaceableTransaction(t)) if t == forged_txid
    );
}

/// `events_helper` reports the rejections of every `apply_*` call made by its closure.
#[test]
fn events_helper_reports_rejections_of_all_calls() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _) = get_funded_wallet(desc, change_desc);
    let tx = signed_payment(&mut wallet, &[desc, change_desc]);
    let forged = forge(&wallet, &tx);
    let mut unsigned = tx.clone();
    for txin in &mut unsigned.input {
        txin.witness.clear();
    }
    let (forged_txid, unsigned_txid) = (forged.compute_txid(), unsigned.compute_txid());

    let events = wallet
        .events_helper::<_, _, core::convert::Infallible>(|wallet| {
            wallet.apply_unconfirmed_txs([(forged, 100)]);
            wallet.apply_unconfirmed_txs([(unsigned, 100)]);
            Ok(())
        })
        .unwrap();
    let mut rejected = rejected_txids(&events);
    rejected.sort();
    let mut expected = vec![forged_txid, unsigned_txid];
    expected.sort();
    assert_eq!(rejected, expected);
}

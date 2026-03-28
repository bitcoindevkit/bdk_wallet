use std::str::FromStr;

use bdk_wallet::psbt::PsbtUtils;
use bdk_wallet::signer::SignOptions;
use bdk_wallet::test_utils::*;
use bdk_wallet::tx_builder::AddForeignUtxoError;
use bdk_wallet::KeychainKind;
use bitcoin::{psbt, Address, Amount};

mod common;

#[test]
fn test_add_foreign_utxo() {
    let (mut wallet1, _) = get_funded_wallet_wpkh();
    let (wallet2, _) =
        get_funded_wallet_single("wpkh(cVbZ8ovhye9AoAHFsqobCf7LxbXDAECy9Kb8TZdfsDYMZGBUyCnm)");

    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let utxo = wallet2.list_unspent().next().expect("must take!");
    let foreign_utxo_satisfaction = wallet2
        .public_descriptor(KeychainKind::External)
        .max_weight_to_satisfy()
        .unwrap();

    let psbt_input = psbt::Input {
        witness_utxo: Some(utxo.txout.clone()),
        ..Default::default()
    };

    wallet1.insert_txout(utxo.outpoint, utxo.txout);

    let mut builder = wallet1.build_tx();
    builder
        .add_recipient(addr.script_pubkey(), Amount::from_sat(60_000))
        .only_witness_utxo()
        .add_foreign_utxo(utxo.outpoint, psbt_input, foreign_utxo_satisfaction)
        .unwrap();
    let mut psbt = builder.finish().unwrap();
    let fee = check_fee!(wallet1, psbt);
    let (sent, received) =
        wallet1.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));

    assert_eq!(
        (sent - received),
        Amount::from_sat(10_000) + fee,
        "we should have only net spent ~10_000"
    );

    assert!(
        psbt.unsigned_tx
            .input
            .iter()
            .any(|input| input.previous_output == utxo.outpoint),
        "foreign_utxo should be in there"
    );

    let finished = wallet1
        .sign(
            &mut psbt,
            SignOptions {
                trust_witness_utxo: true,
                ..Default::default()
            },
        )
        .unwrap();

    assert!(
        !finished,
        "only one of the inputs should have been signed so far"
    );

    let finished = wallet2
        .sign(
            &mut psbt,
            SignOptions {
                trust_witness_utxo: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(finished, "all the inputs should have been signed now");
}

#[test]
fn test_add_foreign_utxo_invalid_psbt_input() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let outpoint = wallet.list_unspent().next().expect("must exist").outpoint;
    let foreign_utxo_satisfaction = wallet
        .public_descriptor(KeychainKind::External)
        .max_weight_to_satisfy()
        .unwrap();

    let mut builder = wallet.build_tx();
    let result =
        builder.add_foreign_utxo(outpoint, psbt::Input::default(), foreign_utxo_satisfaction);
    assert!(matches!(result, Err(AddForeignUtxoError::MissingUtxo)));
}

#[test]
fn test_add_foreign_utxo_requires_inserted_txout() {
    let (mut wallet1, _) = get_funded_wallet_wpkh();
    let (wallet2, _) =
        get_funded_wallet_single("wpkh(cVbZ8ovhye9AoAHFsqobCf7LxbXDAECy9Kb8TZdfsDYMZGBUyCnm)");

    let utxo = wallet2.list_unspent().next().expect("must take!");
    let foreign_utxo_satisfaction = wallet2
        .public_descriptor(KeychainKind::External)
        .max_weight_to_satisfy()
        .unwrap();

    {
        let mut builder = wallet1.build_tx();
        let result = builder.add_foreign_utxo(
            utxo.outpoint,
            psbt::Input {
                witness_utxo: Some(utxo.txout.clone()),
                ..Default::default()
            },
            foreign_utxo_satisfaction,
        );
        assert!(
            matches!(result, Err(AddForeignUtxoError::MissingRegisteredTxOut(outpoint)) if outpoint == utxo.outpoint)
        );
    }

    wallet1.insert_txout(utxo.outpoint, utxo.txout.clone());

    {
        let mut builder = wallet1.build_tx();
        let result = builder.add_foreign_utxo(
            utxo.outpoint,
            psbt::Input {
                witness_utxo: Some(utxo.txout),
                ..Default::default()
            },
            foreign_utxo_satisfaction,
        );
        assert!(
            result.is_ok(),
            "should succeed once the txout is inserted into wallet"
        );
    }
}

#[test]
fn test_add_foreign_utxo_requires_inserted_tx() {
    let (mut wallet1, _) = get_funded_wallet_wpkh();
    let (wallet2, txid2) =
        get_funded_wallet_single("wpkh(cVbZ8ovhye9AoAHFsqobCf7LxbXDAECy9Kb8TZdfsDYMZGBUyCnm)");

    let utxo = wallet2.list_unspent().next().expect("must take!");
    let tx2 = wallet2.get_tx(txid2).unwrap().tx_node.tx;
    let foreign_utxo_satisfaction = wallet2
        .public_descriptor(KeychainKind::External)
        .max_weight_to_satisfy()
        .unwrap();

    {
        let mut builder = wallet1.build_tx();
        let result = builder.add_foreign_utxo(
            utxo.outpoint,
            psbt::Input {
                non_witness_utxo: Some(tx2.as_ref().clone()),
                ..Default::default()
            },
            foreign_utxo_satisfaction,
        );
        assert!(
            matches!(result, Err(AddForeignUtxoError::MissingRegisteredTx(outpoint)) if outpoint == utxo.outpoint)
        );
    }

    wallet1.insert_tx(tx2.as_ref().clone());

    {
        let mut builder = wallet1.build_tx();
        let result = builder.add_foreign_utxo(
            utxo.outpoint,
            psbt::Input {
                non_witness_utxo: Some(tx2.as_ref().clone()),
                ..Default::default()
            },
            foreign_utxo_satisfaction,
        );
        assert!(
            result.is_ok(),
            "should succeed once the parent tx is inserted into wallet"
        );
    }
}

#[test]
/// When both `non_witness_utxo` and `witness_utxo` are present, the parent tx must be inserted.
fn test_add_foreign_utxo_requires_inserted_tx_when_both_prevout_forms_are_present() {
    let (mut wallet1, _) = get_funded_wallet_wpkh();
    let (wallet2, txid2) =
        get_funded_wallet_single("wpkh(cVbZ8ovhye9AoAHFsqobCf7LxbXDAECy9Kb8TZdfsDYMZGBUyCnm)");

    let utxo = wallet2.list_unspent().next().expect("must take!");
    let tx2 = wallet2.get_tx(txid2).unwrap().tx_node.tx;
    let foreign_utxo_satisfaction = wallet2
        .public_descriptor(KeychainKind::External)
        .max_weight_to_satisfy()
        .unwrap();

    wallet1.insert_txout(utxo.outpoint, utxo.txout.clone());

    {
        let mut builder = wallet1.build_tx();
        let result = builder.add_foreign_utxo(
            utxo.outpoint,
            psbt::Input {
                witness_utxo: Some(utxo.txout.clone()),
                non_witness_utxo: Some(tx2.as_ref().clone()),
                ..Default::default()
            },
            foreign_utxo_satisfaction,
        );
        assert!(
            matches!(result, Err(AddForeignUtxoError::MissingRegisteredTx(outpoint)) if outpoint == utxo.outpoint)
        );
    }

    wallet1.insert_tx(tx2.as_ref().clone());

    {
        let mut builder = wallet1.build_tx();
        let result = builder.add_foreign_utxo(
            utxo.outpoint,
            psbt::Input {
                witness_utxo: Some(utxo.txout),
                non_witness_utxo: Some(tx2.as_ref().clone()),
                ..Default::default()
            },
            foreign_utxo_satisfaction,
        );
        assert!(
            result.is_ok(),
            "should require the parent tx when non_witness_utxo is present"
        );
    }
}

#[test]
fn test_add_foreign_utxo_where_outpoint_doesnt_match_psbt_input() {
    let (mut wallet1, txid1) = get_funded_wallet_wpkh();
    let (wallet2, txid2) =
        get_funded_wallet_single("wpkh(cVbZ8ovhye9AoAHFsqobCf7LxbXDAECy9Kb8TZdfsDYMZGBUyCnm)");

    let utxo2 = wallet2.list_unspent().next().unwrap();
    let tx1 = wallet1.get_tx(txid1).unwrap().tx_node.tx.clone();
    let tx2 = wallet2.get_tx(txid2).unwrap().tx_node.tx.clone();

    let satisfaction_weight = wallet2
        .public_descriptor(KeychainKind::External)
        .max_weight_to_satisfy()
        .unwrap();

    {
        let mut builder = wallet1.build_tx();
        assert!(
            builder
                .add_foreign_utxo(
                    utxo2.outpoint,
                    psbt::Input {
                        non_witness_utxo: Some(tx1.as_ref().clone()),
                        ..Default::default()
                    },
                    satisfaction_weight
                )
                .is_err(),
            "should fail when outpoint doesn't match psbt_input"
        );
    }
    wallet1.insert_tx(tx2.as_ref().clone());
    {
        let mut builder = wallet1.build_tx();
        assert!(
            builder
                .add_foreign_utxo(
                    utxo2.outpoint,
                    psbt::Input {
                        non_witness_utxo: Some(tx2.as_ref().clone()),
                        ..Default::default()
                    },
                    satisfaction_weight
                )
                .is_ok(),
            "should be ok when outpoint does match psbt_input"
        );
    }
}

#[test]
fn test_add_foreign_utxo_only_witness_utxo() {
    let (mut wallet1, _) = get_funded_wallet_wpkh();
    let (wallet2, txid2) =
        get_funded_wallet_single("wpkh(cVbZ8ovhye9AoAHFsqobCf7LxbXDAECy9Kb8TZdfsDYMZGBUyCnm)");
    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let utxo2 = wallet2.list_unspent().next().unwrap();

    let satisfaction_weight = wallet2
        .public_descriptor(KeychainKind::External)
        .max_weight_to_satisfy()
        .unwrap();

    wallet1.insert_txout(utxo2.outpoint, utxo2.txout.clone());

    {
        let mut builder = wallet1.build_tx();
        builder.add_recipient(addr.script_pubkey(), Amount::from_sat(60_000));

        let psbt_input = psbt::Input {
            witness_utxo: Some(utxo2.txout.clone()),
            ..Default::default()
        };
        builder
            .add_foreign_utxo(utxo2.outpoint, psbt_input, satisfaction_weight)
            .unwrap();
        assert!(
            builder.finish().is_err(),
            "psbt_input with witness_utxo should fail with only witness_utxo"
        );
    }

    {
        let mut builder = wallet1.build_tx();
        builder.add_recipient(addr.script_pubkey(), Amount::from_sat(60_000));

        let psbt_input = psbt::Input {
            witness_utxo: Some(utxo2.txout.clone()),
            ..Default::default()
        };
        builder
            .only_witness_utxo()
            .add_foreign_utxo(utxo2.outpoint, psbt_input, satisfaction_weight)
            .unwrap();
        assert!(
            builder.finish().is_ok(),
            "psbt_input with just witness_utxo should succeed when `only_witness_utxo` is enabled"
        );
    }

    {
        let tx2 = wallet2.get_tx(txid2).unwrap().tx_node.tx;
        wallet1.insert_tx(tx2.as_ref().clone());

        let mut builder = wallet1.build_tx();
        builder.add_recipient(addr.script_pubkey(), Amount::from_sat(60_000));

        let psbt_input = psbt::Input {
            non_witness_utxo: Some(tx2.as_ref().clone()),
            ..Default::default()
        };
        builder
            .add_foreign_utxo(utxo2.outpoint, psbt_input, satisfaction_weight)
            .unwrap();
        assert!(
            builder.finish().is_ok(),
            "psbt_input with non_witness_utxo should succeed by default"
        );
    }
}

#[test]
fn test_taproot_foreign_utxo() {
    let (mut wallet1, _) = get_funded_wallet_wpkh();
    let (wallet2, _) = get_funded_wallet_single(get_test_tr_single_sig());

    let addr = Address::from_str("2N1Ffz3WaNzbeLFBb51xyFMHYSEUXcbiSoX")
        .unwrap()
        .assume_checked();
    let utxo = wallet2.list_unspent().next().unwrap();
    let psbt_input = wallet2.get_psbt_input(utxo.clone(), None, false).unwrap();
    let foreign_utxo_satisfaction = wallet2
        .public_descriptor(KeychainKind::External)
        .max_weight_to_satisfy()
        .unwrap();

    assert!(
        psbt_input.non_witness_utxo.is_none(),
        "`non_witness_utxo` should never be populated for taproot"
    );

    wallet1.insert_txout(utxo.outpoint, utxo.txout);

    let mut builder = wallet1.build_tx();
    builder
        .add_recipient(addr.script_pubkey(), Amount::from_sat(60_000))
        .add_foreign_utxo(utxo.outpoint, psbt_input, foreign_utxo_satisfaction)
        .unwrap();
    let psbt = builder.finish().unwrap();
    let (sent, received) =
        wallet1.sent_and_received(&psbt.clone().extract_tx().expect("failed to extract tx"));
    let fee = check_fee!(wallet1, psbt);

    assert_eq!(
        sent - received,
        Amount::from_sat(10_000) + fee,
        "we should have only net spent ~10_000"
    );

    assert!(
        psbt.unsigned_tx
            .input
            .iter()
            .any(|input| input.previous_output == utxo.outpoint),
        "foreign_utxo should be in there"
    );
}

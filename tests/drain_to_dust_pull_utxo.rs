use bdk_wallet::test_utils::*;
use bdk_wallet::KeychainKind;
use bitcoin::{hashes::Hash, psbt, Amount, OutPoint, ScriptBuf, TxOut, Weight};

// Ensures coin selection pulls a local UTXO when drain-only selection would produce dust.
#[test]
fn test_drain_to_pulls_local_utxo_when_foreign_only_dust() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let drain_spk = wallet
        .next_unused_address(KeychainKind::External)
        .script_pubkey();

    let witness_utxo = TxOut {
        value: Amount::from_sat(500),
        script_pubkey: ScriptBuf::new_p2a(),
    };
    // Remember to include this as a "floating" txout in the wallet.
    let outpoint = OutPoint::new(Hash::hash(b"foreign-p2a-prev"), 1);
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
        .fee_absolute(Amount::from_sat(400))
        .drain_to(drain_spk);

    let psbt = tx_builder.finish().unwrap();
    let tx = psbt.unsigned_tx;
    assert!(tx.input.len() >= 2);
    assert!(!tx.output.is_empty());
    assert!(
        tx.input.iter().any(|txin| txin.previous_output == outpoint),
        "foreign_utxo should be in there"
    );
}

// Foreign value equals fee: no satoshis left for a drain output until a wallet UTXO is included.
#[test]
fn test_drain_to_pulls_local_utxo_when_foreign_value_equals_fee() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let drain_spk = wallet
        .next_unused_address(KeychainKind::External)
        .script_pubkey();

    let witness_utxo = TxOut {
        value: Amount::from_sat(200),
        script_pubkey: ScriptBuf::new_p2a(),
    };
    let outpoint = OutPoint::new(Hash::hash(b"foreign-p2a-prev-200"), 1);
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
        .fee_absolute(Amount::from_sat(200))
        .drain_to(drain_spk);

    let psbt = tx_builder.finish().unwrap();
    let tx = psbt.unsigned_tx;
    assert!(tx.input.len() >= 2);
    assert!(!tx.output.is_empty());
    assert!(tx.input.iter().any(|txin| txin.previous_output == outpoint));
}

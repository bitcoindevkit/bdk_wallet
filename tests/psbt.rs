use bdk_wallet::bitcoin::bip32::Xpriv;
use bdk_wallet::bitcoin::psbt::SigningKeys;
use bdk_wallet::test_utils::*;
use bdk_wallet::{KeychainKind, psbt};
use bitcoin::{Amount, FeeRate, Psbt, TxIn, Witness};
use core::str::FromStr;
use miniscript::descriptor::KeyMapWrapper;
use miniscript::psbt::PsbtExt;

mod common;
use common::keymap_from_descriptor;

#[test]
fn test_psbt_sign_with_finalized() {
    let (descriptor, change_descriptor) = get_test_wpkh_and_change_desc();
    let (mut wallet, _) = get_funded_wallet(descriptor, change_descriptor);
    let (foreign, foreign_txid) = get_funded_wallet_single(get_test_wpkh());
    let send_to = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.add_recipient(send_to.script_pubkey(), Amount::from_sat(10_000));
    let mut psbt = builder.finish().unwrap();

    let foreign_utxo = foreign.list_unspent().next().unwrap();
    let foreign_tx = foreign
        .get_tx(foreign_txid)
        .unwrap()
        .tx_node
        .tx
        .as_ref()
        .clone();
    psbt.inputs.push(bitcoin::psbt::Input {
        non_witness_utxo: Some(foreign_tx),
        witness_utxo: Some(foreign_utxo.txout),
        final_script_witness: Some(Witness::from_slice(&[&[0x01]])),
        ..Default::default()
    });
    psbt.unsigned_tx.input.push(TxIn {
        previous_output: foreign_utxo.outpoint,
        ..Default::default()
    });

    let mut keymap = keymap_from_descriptor(&wallet, descriptor);
    keymap.extend(keymap_from_descriptor(&wallet, change_descriptor));
    let signer = KeyMapWrapper::from(keymap);
    let foreign_index = psbt.inputs.len() - 1;
    let foreign_input_before = psbt.inputs[foreign_index].clone();

    psbt.sign(&signer, wallet.secp_ctx()).unwrap();

    assert!(
        psbt.inputs[..foreign_index]
            .iter()
            .all(|input| !input.partial_sigs.is_empty())
    );
    assert!(wallet.finalize_psbt(&mut psbt).unwrap());
    assert_eq!(psbt.inputs[foreign_index], foreign_input_before);
}

#[test]
fn test_psbt_fee_rate_with_witness_utxo() {
    use psbt::PsbtUtils;

    let expected_fee_rate = FeeRate::from_sat_per_kwu(310);

    let descriptor = "wpkh(tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS/*)";
    let (mut wallet, _) = get_funded_wallet_single(descriptor);
    let addr = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.drain_to(addr.script_pubkey()).drain_wallet();
    builder.fee_rate(expected_fee_rate);
    let mut psbt = builder.finish().unwrap();
    let fee_amount = psbt.fee_amount();
    assert!(fee_amount.is_some());

    let unfinalized_fee_rate = psbt.fee_rate().unwrap();

    let signer = KeyMapWrapper::from(keymap_from_descriptor(&wallet, descriptor));
    psbt.sign(&signer, wallet.secp_ctx()).unwrap();
    let finalized = wallet.finalize_psbt(&mut psbt).unwrap();
    assert!(finalized);

    let finalized_fee_rate = psbt.fee_rate().unwrap();
    assert!(finalized_fee_rate >= expected_fee_rate);
    assert!(finalized_fee_rate < unfinalized_fee_rate);
}

#[test]
fn test_psbt_fee_rate_with_nonwitness_utxo() {
    use psbt::PsbtUtils;

    let expected_fee_rate = FeeRate::from_sat_per_kwu(310);

    let descriptor = "pkh(tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS/*)";
    let (mut wallet, _) = get_funded_wallet_single(descriptor);
    let addr = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.drain_to(addr.script_pubkey()).drain_wallet();
    builder.fee_rate(expected_fee_rate);
    let mut psbt = builder.finish().unwrap();
    let fee_amount = psbt.fee_amount();
    assert!(fee_amount.is_some());
    let unfinalized_fee_rate = psbt.fee_rate().unwrap();

    let signer = KeyMapWrapper::from(keymap_from_descriptor(&wallet, descriptor));
    psbt.sign(&signer, wallet.secp_ctx()).unwrap();
    let finalized = wallet.finalize_psbt(&mut psbt).unwrap();
    assert!(finalized);

    let finalized_fee_rate = psbt.fee_rate().unwrap();
    assert!(finalized_fee_rate >= expected_fee_rate);
    assert!(finalized_fee_rate < unfinalized_fee_rate);
}

#[test]
fn test_psbt_fee_rate_with_missing_txout() {
    use psbt::PsbtUtils;

    let expected_fee_rate = FeeRate::from_sat_per_kwu(310);

    let (mut wpkh_wallet, _) = get_funded_wallet_single(
        "wpkh(tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS/*)",
    );
    let addr = wpkh_wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wpkh_wallet.build_tx();
    builder.drain_to(addr.script_pubkey()).drain_wallet();
    builder.fee_rate(expected_fee_rate);
    let mut wpkh_psbt = builder.finish().unwrap();

    wpkh_psbt.inputs[0].witness_utxo = None;
    wpkh_psbt.inputs[0].non_witness_utxo = None;
    assert!(wpkh_psbt.fee_amount().is_none());
    assert!(wpkh_psbt.fee_rate().is_none());

    let desc = "pkh(tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS/0)";
    let change_desc = "pkh(tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS/1)";
    let (mut pkh_wallet, _) = get_funded_wallet(desc, change_desc);
    let addr = pkh_wallet.peek_address(KeychainKind::External, 0);
    let mut builder = pkh_wallet.build_tx();
    builder.drain_to(addr.script_pubkey()).drain_wallet();
    builder.fee_rate(expected_fee_rate);
    let mut pkh_psbt = builder.finish().unwrap();

    pkh_psbt.inputs[0].non_witness_utxo = None;
    assert!(pkh_psbt.fee_amount().is_none());
    assert!(pkh_psbt.fee_rate().is_none());
}

// wpkh PSBT with bip32_derivation populated, derived from
// tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L
const WPKH_PSBT_WITH_DERIVATION: &str = "cHNidP8BAHECAAAAAbOlV/kRKdNVk6Wn2cay5JUvpFw4tEsKWylqu+HfPKDyAAAAAAD9////ArObAAAAAAAAFgAU2+Ijq+8PDcPUGgHQqOPg9+6n9h8QJwAAAAAAABYAFKENkldInmhd2gMGYjkNwXeFL68T0AcAAAABAHEBAAAAATCzgdcz18YZK+8oNpJzqjM8ErFYW3hJLi+bO4bjmQrRAAAAAAD/////AlDDAAAAAAAAFgAUoQ2SV0ieaF3aAwZiOQ3Bd4UvrxOoYQAAAAAAABYAFIgWLNSQrRaGsRyWtCHaqeauCPYsAAAAAAEBH1DDAAAAAAAAFgAUoQ2SV0ieaF3aAwZiOQ3Bd4UvrxMiBgLOtp4iMz+DVWxdHvunWgM0a/PVLPvTn8XSTe0DTvfZ9Bjic/5CVAAAgAEAAIAAAACAAAAAAAAAAAAAIgIDxWYngmqPgL3saGZ4NTgcy5W/XINU8lkqnqjKC+oJuRwY4nP+QlQAAIABAACAAAAAgAEAAAAAAAAAACICAs62niIzP4NVbF0e+6daAzRr89Us+9OfxdJN7QNO99n0GOJz/kJUAACAAQAAgAAAAIAAAAAAAAAAAAA=";

#[test]
fn test_psbt_sign_with_xpriv() {
    let mut psbt = Psbt::from_str(WPKH_PSBT_WITH_DERIVATION).unwrap();
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (wallet, _) = get_funded_wallet(desc, change_desc);

    let xpriv = Xpriv::from_str(
        "tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L",
    )
    .unwrap();

    let signing_keys = psbt
        .sign(&xpriv, wallet.secp_ctx())
        .expect("PSBT signing should succeed with the correct xpriv");

    assert!(
        !signing_keys.is_empty(),
        "expected at least one input to be signed"
    );
}

#[test]
fn test_psbt_sign_with_wrong_key_signs_nothing() {
    let mut psbt = Psbt::from_str(WPKH_PSBT_WITH_DERIVATION).unwrap();
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (wallet, _) = get_funded_wallet(desc, change_desc);

    let wrong_xpriv = Xpriv::from_str(
        "tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS",
    )
    .unwrap();

    let signing_keys = psbt
        .sign(&wrong_xpriv, wallet.secp_ctx())
        .expect("PSBT signing returns Ok even when no keys matched");

    let keys_used: usize = signing_keys
        .values()
        .map(|keys| match keys {
            SigningKeys::Ecdsa(v) => v.len(),
            SigningKeys::Schnorr(v) => v.len(),
        })
        .sum();
    assert_eq!(
        keys_used, 0,
        "expected zero keys used when signing with an unrelated xpriv"
    );
    for input in &psbt.inputs {
        assert!(
            input.partial_sigs.is_empty(),
            "expected no partial signatures when signing with an unrelated key"
        );
    }
}

#[test]
fn test_psbt_sign_with_descriptor_keymap() {
    let mut psbt = Psbt::from_str(WPKH_PSBT_WITH_DERIVATION).unwrap();
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (wallet, _) = get_funded_wallet(desc, change_desc);

    let mut keymap = keymap_from_descriptor(&wallet, desc);
    keymap.extend(keymap_from_descriptor(&wallet, change_desc));
    let wrapper = KeyMapWrapper::from(keymap);

    let signing_keys = psbt
        .sign(&wrapper, wallet.secp_ctx())
        .expect("PSBT signing should succeed with descriptor keymap");

    assert!(
        !signing_keys.is_empty(),
        "expected at least one input to be signed using descriptor keymap"
    );
}

// Signing with an account-level xprv must derive `xprv/0/0`, not `xprv/86h/1h/0h/0/0`.
#[test]
fn test_psbt_sign_with_descriptor_keymap_key_origin() {
    let descriptor = "tr([5159c173/86h/1h/0h]tprv8gfBCLYgHZLJLhg1DUSY6NN6eNAee3aqt3SE3M1hywpAiFWYGsm6U6B17VDKh8W1SQnatRpZujU5xseJ84nYZmkxPGCngNhmBMVRY2sAZhY/0/*)";
    let (mut wallet, _) = get_funded_wallet_single(descriptor);
    let addr = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.drain_to(addr.script_pubkey()).drain_wallet();
    let mut psbt = builder.finish().unwrap();

    let signer = KeyMapWrapper::from(keymap_from_descriptor(&wallet, descriptor));
    psbt.sign(&signer, wallet.secp_ctx()).unwrap();
    psbt.finalize_mut(wallet.secp_ctx())
        .expect("signature must be valid");
}

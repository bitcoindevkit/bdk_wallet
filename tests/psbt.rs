use bdk_chain::{BlockId, ConfirmationBlockTime};
use bdk_wallet::bitcoin;
use bdk_wallet::test_utils::*;
use bdk_wallet::{error::CreatePsbtError, psbt, KeychainKind, PsbtParams, SignOptions, Wallet};
use bitcoin::{
    absolute, hashes::Hash, secp256k1, Address, Amount, FeeRate, Network, OutPoint, Psbt,
    ScriptBuf, Transaction, TxIn, TxOut,
};
use core::str::FromStr;
use miniscript::plan::Assets;
use std::sync::Arc;

// from bip 174
const PSBT_STR: &str = "cHNidP8BAKACAAAAAqsJSaCMWvfEm4IS9Bfi8Vqz9cM9zxU4IagTn4d6W3vkAAAAAAD+////qwlJoIxa98SbghL0F+LxWrP1wz3PFTghqBOfh3pbe+QBAAAAAP7///8CYDvqCwAAAAAZdqkUdopAu9dAy+gdmI5x3ipNXHE5ax2IrI4kAAAAAAAAGXapFG9GILVT+glechue4O/p+gOcykWXiKwAAAAAAAEHakcwRAIgR1lmF5fAGwNrJZKJSGhiGDR9iYZLcZ4ff89X0eURZYcCIFMJ6r9Wqk2Ikf/REf3xM286KdqGbX+EhtdVRs7tr5MZASEDXNxh/HupccC1AaZGoqg7ECy0OIEhfKaC3Ibi1z+ogpIAAQEgAOH1BQAAAAAXqRQ1RebjO4MsRwUPJNPuuTycA5SLx4cBBBYAFIXRNTfy4mVAWjTbr6nj3aAfuCMIAAAA";

#[test]
fn test_create_psbt() {
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

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

    let change_desc =
        miniscript::Descriptor::parse_descriptor(&secp256k1::Secp256k1::new(), change_desc)
            .unwrap()
            .0
            .at_derivation_index(0)
            .unwrap();

    let addr = wallet.reveal_next_address(KeychainKind::External);
    let mut params = PsbtParams::default();
    params
        .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())])
        .change_descriptor(change_desc)
        .feerate(FeeRate::from_sat_per_vb_unchecked(4));

    let (psbt, _) = wallet.create_psbt(params).unwrap();
    assert!(psbt.fee().is_ok());
    let psbt_input = &psbt.inputs[0];
    assert_eq!(
        psbt_input.witness_utxo.as_ref().map(|txo| txo.value),
        Some(Amount::ONE_BTC),
    );
    assert!(psbt_input.tap_internal_key.is_some());
    assert!(psbt_input
        .tap_key_origins
        .values()
        .any(|(_, (fp, _))| fp.to_string() == "f6a5cb8b"));
    assert!(psbt
        .outputs
        .iter()
        .any(|output| output.tap_internal_key.is_some()));
    assert!(psbt.outputs.iter().any(|output| output
        .tap_key_origins
        .values()
        .any(|(_, (fp, _))| fp.to_string() == "f6a5cb8b")));
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

    // FIXME: Locktime greater than required
    {
        let mut params = PsbtParams::default();
        params
            .add_utxos(&[op])
            .locktime(LockTime::from_consensus(200_000))
            .add_recipients([(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())]);

        // let (psbt, _) = wallet.create_psbt(params).unwrap();
        // assert_eq!(psbt.unsigned_tx.lock_time.to_consensus_u32(), 200_000);
    }
}

// Test that replacing two unconfirmed txs A, B results in a transaction
// that spends the inputs of both A and B.
#[test]
fn test_replace_by_fee() {
    use KeychainKind::*;
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    // The anchor block
    let block = BlockId {
        height: 100,
        hash: Hash::hash(b"100"),
    };

    let mut addrs: Vec<Address> = vec![];
    for _ in 0..3 {
        let addr = wallet.reveal_next_address(External);
        addrs.push(addr.address);
    }

    // Insert parent 0 (coinbase)
    let p0 = Transaction {
        input: vec![TxIn::default()],
        output: vec![TxOut {
            value: Amount::ONE_BTC,
            script_pubkey: addrs[0].script_pubkey(),
        }],
        ..new_tx(1)
    };
    let op0 = OutPoint::new(p0.compute_txid(), 0);

    insert_tx_anchor(&mut wallet, p0.clone(), block);

    // Insert parent 1 (coinbase)
    let p1 = Transaction {
        input: vec![TxIn::default()],
        output: vec![TxOut {
            value: Amount::ONE_BTC,
            script_pubkey: addrs[1].script_pubkey(),
        }],
        ..new_tx(1)
    };
    let op1 = OutPoint::new(p1.compute_txid(), 0);

    insert_tx_anchor(&mut wallet, p1.clone(), block);

    // Add new tip, for maturity
    let block = BlockId {
        height: 1000,
        hash: Hash::hash(b"1000"),
    };
    insert_checkpoint(&mut wallet, block);

    // Create tx A (unconfirmed)
    let recip =
        ScriptBuf::from_hex("5120e8f5c4dc2f5d6a7595e7b108cb063da9c7550312da1e22875d78b9db62b59cd5")
            .unwrap();
    let mut params = PsbtParams::default();
    params
        .add_utxos(&[op0])
        .add_recipients([(recip.clone(), Amount::from_sat(16_000))]);
    let txa = wallet.create_psbt(params).unwrap().0.unsigned_tx;
    insert_tx(&mut wallet, txa.clone());

    // Create tx B (unconfirmed)
    let mut params = PsbtParams::default();
    params
        .add_utxos(&[op1])
        .add_recipients([(recip.clone(), Amount::from_sat(42_000))]);
    let txb = wallet.create_psbt(params).unwrap().0.unsigned_tx;
    insert_tx(&mut wallet, txb.clone());

    // Now create RBF tx
    let psbt = wallet
        .replace_by_fee_and_recipients(
            &[Arc::new(txa), Arc::new(txb)],
            FeeRate::from_sat_per_vb_unchecked(4),
            vec![(recip, Amount::from_btc(1.99).unwrap())],
        )
        .unwrap()
        .0;

    // Expect replace inputs of A, B
    assert_eq!(
        psbt.unsigned_tx.input.len(),
        2,
        "We should have selected two inputs"
    );
    for op in [op0, op1] {
        assert!(
            psbt.unsigned_tx
                .input
                .iter()
                .any(|txin| txin.previous_output == op),
            "We should have replaced the original spends"
        );
    }
}

#[test]
#[should_panic(expected = "InputIndexOutOfRange")]
fn test_psbt_malformed_psbt_input_legacy() {
    let psbt_bip = Psbt::from_str(PSBT_STR).unwrap();
    let (mut wallet, _) = get_funded_wallet_single(get_test_wpkh());
    let send_to = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.add_recipient(send_to.script_pubkey(), Amount::from_sat(10_000));
    let mut psbt = builder.finish().unwrap();
    psbt.inputs.push(psbt_bip.inputs[0].clone());
    let options = SignOptions {
        trust_witness_utxo: true,
        ..Default::default()
    };
    let _ = wallet.sign(&mut psbt, options).unwrap();
}

#[test]
#[should_panic(expected = "InputIndexOutOfRange")]
fn test_psbt_malformed_psbt_input_segwit() {
    let psbt_bip = Psbt::from_str(PSBT_STR).unwrap();
    let (mut wallet, _) = get_funded_wallet_single(get_test_wpkh());
    let send_to = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.add_recipient(send_to.script_pubkey(), Amount::from_sat(10_000));
    let mut psbt = builder.finish().unwrap();
    psbt.inputs.push(psbt_bip.inputs[1].clone());
    let options = SignOptions {
        trust_witness_utxo: true,
        ..Default::default()
    };
    let _ = wallet.sign(&mut psbt, options).unwrap();
}

#[test]
#[should_panic(expected = "InputIndexOutOfRange")]
fn test_psbt_malformed_tx_input() {
    let (mut wallet, _) = get_funded_wallet_single(get_test_wpkh());
    let send_to = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.add_recipient(send_to.script_pubkey(), Amount::from_sat(10_000));
    let mut psbt = builder.finish().unwrap();
    psbt.unsigned_tx.input.push(TxIn::default());
    let options = SignOptions {
        trust_witness_utxo: true,
        ..Default::default()
    };
    let _ = wallet.sign(&mut psbt, options).unwrap();
}

#[test]
fn test_psbt_sign_with_finalized() {
    let psbt_bip = Psbt::from_str(PSBT_STR).unwrap();
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let send_to = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.add_recipient(send_to.script_pubkey(), Amount::from_sat(10_000));
    let mut psbt = builder.finish().unwrap();

    // add a finalized input
    psbt.inputs.push(psbt_bip.inputs[0].clone());
    psbt.unsigned_tx
        .input
        .push(psbt_bip.unsigned_tx.input[0].clone());

    let _ = wallet.sign(&mut psbt, SignOptions::default()).unwrap();
}

#[test]
fn test_psbt_fee_rate_with_witness_utxo() {
    use psbt::PsbtUtils;

    let expected_fee_rate = FeeRate::from_sat_per_kwu(310);

    let (mut wallet, _) = get_funded_wallet_single("wpkh(tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS/*)");
    let addr = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.drain_to(addr.script_pubkey()).drain_wallet();
    builder.fee_rate(expected_fee_rate);
    let mut psbt = builder.finish().unwrap();
    let fee_amount = psbt.fee_amount();
    assert!(fee_amount.is_some());

    let unfinalized_fee_rate = psbt.fee_rate().unwrap();

    let finalized = wallet.sign(&mut psbt, Default::default()).unwrap();
    assert!(finalized);

    let finalized_fee_rate = psbt.fee_rate().unwrap();
    assert!(finalized_fee_rate >= expected_fee_rate);
    assert!(finalized_fee_rate < unfinalized_fee_rate);
}

#[test]
fn test_psbt_fee_rate_with_nonwitness_utxo() {
    use psbt::PsbtUtils;

    let expected_fee_rate = FeeRate::from_sat_per_kwu(310);

    let (mut wallet, _) = get_funded_wallet_single("pkh(tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS/*)");
    let addr = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.drain_to(addr.script_pubkey()).drain_wallet();
    builder.fee_rate(expected_fee_rate);
    let mut psbt = builder.finish().unwrap();
    let fee_amount = psbt.fee_amount();
    assert!(fee_amount.is_some());
    let unfinalized_fee_rate = psbt.fee_rate().unwrap();

    let finalized = wallet.sign(&mut psbt, Default::default()).unwrap();
    assert!(finalized);

    let finalized_fee_rate = psbt.fee_rate().unwrap();
    assert!(finalized_fee_rate >= expected_fee_rate);
    assert!(finalized_fee_rate < unfinalized_fee_rate);
}

#[test]
fn test_psbt_fee_rate_with_missing_txout() {
    use psbt::PsbtUtils;

    let expected_fee_rate = FeeRate::from_sat_per_kwu(310);

    let (mut wpkh_wallet,  _) = get_funded_wallet_single("wpkh(tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS/*)");
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

#[test]
fn test_psbt_multiple_internalkey_signers() {
    use bdk_wallet::signer::{SignerContext, SignerOrdering, SignerWrapper};
    use bdk_wallet::KeychainKind;
    use bitcoin::key::TapTweak;
    use bitcoin::secp256k1::{schnorr, Keypair, Message, Secp256k1, XOnlyPublicKey};
    use bitcoin::sighash::{Prevouts, SighashCache, TapSighashType};
    use bitcoin::{PrivateKey, TxOut};
    use std::sync::Arc;

    let secp = Secp256k1::new();
    let wif = "cNJmN3fH9DDbDt131fQNkVakkpzawJBSeybCUNmP1BovpmGQ45xG";
    let desc = format!("tr({wif})");
    let prv = PrivateKey::from_wif(wif).unwrap();
    let keypair = Keypair::from_secret_key(&secp, &prv.inner);

    let change_desc = "tr(cVpPVruEDdmutPzisEsYvtST1usBR3ntr8pXSyt6D2YYqXRyPcFW)";
    let (mut wallet, _) = get_funded_wallet(&desc, change_desc);
    let to_spend = wallet.balance().total();
    let send_to = wallet.peek_address(KeychainKind::External, 0);
    let mut builder = wallet.build_tx();
    builder.drain_to(send_to.script_pubkey()).drain_wallet();
    let mut psbt = builder.finish().unwrap();
    let unsigned_tx = psbt.unsigned_tx.clone();

    // Adds a signer for the wrong internal key, bdk should not use this key to sign
    wallet.add_signer(
        KeychainKind::External,
        // A signerordering lower than 100, bdk will use this signer first
        SignerOrdering(0),
        Arc::new(SignerWrapper::new(
            PrivateKey::from_wif("5J5PZqvCe1uThJ3FZeUUFLCh2FuK9pZhtEK4MzhNmugqTmxCdwE").unwrap(),
            SignerContext::Tap {
                is_internal_key: true,
            },
        )),
    );
    let finalized = wallet.sign(&mut psbt, SignOptions::default()).unwrap();
    assert!(finalized);

    // To verify, we need the signature, message, and pubkey
    let witness = psbt.inputs[0].final_script_witness.as_ref().unwrap();
    assert!(!witness.is_empty());
    let signature = schnorr::Signature::from_slice(witness.iter().next().unwrap()).unwrap();

    // the prevout we're spending
    let prevouts = &[TxOut {
        script_pubkey: send_to.script_pubkey(),
        value: to_spend,
    }];
    let prevouts = Prevouts::All(prevouts);
    let input_index = 0;
    let mut sighash_cache = SighashCache::new(unsigned_tx);
    let sighash = sighash_cache
        .taproot_key_spend_signature_hash(input_index, &prevouts, TapSighashType::Default)
        .unwrap();
    let message = Message::from(sighash);

    // add tweak. this was taken from `signer::sign_psbt_schnorr`
    let keypair = keypair.tap_tweak(&secp, None).to_keypair();
    let (xonlykey, _parity) = XOnlyPublicKey::from_keypair(&keypair);

    // Must verify if we used the correct key to sign
    let verify_res = secp.verify_schnorr(&signature, &message, &xonlykey);
    assert!(verify_res.is_ok(), "The wrong internal key was used");
}

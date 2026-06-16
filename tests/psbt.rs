use bdk_chain::{BlockId, ConfirmationBlockTime};
use bdk_tx::bdk_coin_select;
use bdk_tx::ChangeScript;
use bdk_wallet::bitcoin;
use bdk_wallet::test_utils::*;
use bdk_wallet::{
    error::{CandidatesError, CreatePsbtError},
    psbt,
    psbt::FinishParams,
    CandidateParams, KeychainKind, SelectParams, SignOptions, Wallet,
};
use bitcoin::{
    absolute, hashes::Hash, Address, Amount, FeeRate, Network, OutPoint, Psbt, ScriptBuf, Sequence,
    Transaction, TxIn, TxOut,
};
use core::str::FromStr;
use miniscript::plan::Assets;

// from bip 174
const PSBT_STR: &str = "cHNidP8BAKACAAAAAqsJSaCMWvfEm4IS9Bfi8Vqz9cM9zxU4IagTn4d6W3vkAAAAAAD+////qwlJoIxa98SbghL0F+LxWrP1wz3PFTghqBOfh3pbe+QBAAAAAP7///8CYDvqCwAAAAAZdqkUdopAu9dAy+gdmI5x3ipNXHE5ax2IrI4kAAAAAAAAGXapFG9GILVT+glechue4O/p+gOcykWXiKwAAAAAAAEHakcwRAIgR1lmF5fAGwNrJZKJSGhiGDR9iYZLcZ4ff89X0eURZYcCIFMJ6r9Wqk2Ikf/REf3xM286KdqGbX+EhtdVRs7tr5MZASEDXNxh/HupccC1AaZGoqg7ECy0OIEhfKaC3Ibi1z+ogpIAAQEgAOH1BQAAAAAXqRQ1RebjO4MsRwUPJNPuuTycA5SLx4cBBBYAFIXRNTfy4mVAWjTbr6nj3aAfuCMIAAAA";

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
    let mut params = SelectParams::new();
    let feerate = FeeRate::from_sat_per_vb(4).unwrap();
    let selection_strategy = psbt::SelectionStrategy::LowestFee {
        longterm_feerate: FeeRate::from_sat_per_vb(2).unwrap(),
        max_rounds: 1000,
    };
    params.coin_selection = selection_strategy;
    params.recipients = vec![(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())];
    params.change_script = Some(ChangeScript::from_descriptor(change_descriptor));
    params.fee_rate = feerate;

    let coins = wallet.candidates().unwrap();
    let template = wallet
        .select(coins, params)
        .unwrap()
        .set_version(bitcoin::transaction::Version(3))
        .unwrap();
    let finish_params = FinishParams {
        add_global_xpubs: true,
        ..Default::default()
    };
    let (psbt, _) = wallet.finish(template, finish_params).unwrap();
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

    let mut params = SelectParams::new();
    params.recipients = vec![(addr.script_pubkey(), Amount::from_sat(10_000))];

    let coins = wallet.candidates().unwrap();
    let result = wallet.select(coins, params);
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
    let mut cp = CandidateParams::new();
    cp.maturity_height = Some(bitcoin::absolute::Height::from_consensus(99).unwrap());
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut p = SelectParams::new();
    p.recipients = vec![(send_to_address.script_pubkey(), Amount::from_sat(58_000))];

    let _ = wallet
        .select(coins, p)
        .expect_err("immature output must not be selected");

    // We can use the params to coerce the coinbase maturity.
    let mut cp = CandidateParams::new();
    cp.maturity_height = Some(bitcoin::absolute::Height::from_consensus(100).unwrap());
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut p = SelectParams::new();
    p.recipients = vec![(send_to_address.script_pubkey(), Amount::from_sat(58_000))];

    let _ = wallet
        .select(coins, p)
        .expect("`maturity_height` should enable selection");

    // The output is eligible for selection once the wallet tip reaches maturity height minus 1
    // (100), as it can be confirmed in the next block (101).
    let block_100 = BlockId {
        height: 100,
        hash: Hash::hash(b"100"),
    };
    insert_checkpoint(&mut wallet, block_100);
    let coins = wallet.candidates().unwrap();
    let mut p = SelectParams::new();
    p.recipients = vec![(send_to_address.script_pubkey(), Amount::from_sat(58_000))];

    let _ = wallet
        .select(coins, p)
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
        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        let res = wallet.candidates_with(&cp);
        assert!(
            matches!(res, Err(CandidatesError::Plan(err)) if err == op),
            "UTXO requires CLTV but the assets are insufficient",
        );
    }

    // Add assets ok
    {
        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        cp.assets = Assets::new().after(LockTime::from_consensus(100_000));
        let coins = wallet.candidates_with(&cp).unwrap();
        let mut params = SelectParams::new();
        params.recipients = vec![(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())];
        let template = wallet.select(coins, params).unwrap();
        let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();
        assert_eq!(psbt.unsigned_tx.lock_time.to_consensus_u32(), 100_000);
    }

    // New chain tip (no assets) ok
    {
        let block_id = BlockId {
            height: 100_000,
            hash: Hash::hash(b"123"),
        };
        insert_checkpoint(&mut wallet, block_id);

        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        let coins = wallet.candidates_with(&cp).unwrap();
        let mut params = SelectParams::new();
        params.recipients = vec![(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())];
        let template = wallet.select(coins, params).unwrap();
        let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();
        assert_eq!(psbt.unsigned_tx.lock_time.to_consensus_u32(), 100_000);
    }

    // Locktime greater than required
    {
        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        let coins = wallet.candidates_with(&cp).unwrap();
        let mut params = SelectParams::new();
        params.recipients = vec![(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())];

        let template = wallet
            .select(coins, params)
            .unwrap()
            .set_locktime(LockTime::from_consensus(200_000))
            .unwrap();
        let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();
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
        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        let res = wallet.candidates_with(&cp);
        assert!(
            matches!(res, Err(CandidatesError::Plan(err)) if err == op),
            "UTXO requires CLTV but the assets are insufficient",
        );
    }

    // Add assets ok
    {
        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        cp.assets = Assets::new().after(lock_time);
        let coins = wallet.candidates_with(&cp).unwrap();
        let mut params = SelectParams::new();
        params.recipients = vec![(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())];
        let template = wallet.select(coins, params).unwrap();
        let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();
        assert_eq!(psbt.unsigned_tx.lock_time, lock_time);
    }

    // Locktime greater than required
    {
        let new_lock_time = 1772167108;
        assert!(new_lock_time > lock_time.to_consensus_u32());
        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        cp.assets = Assets::new().after(lock_time);
        let coins = wallet.candidates_with(&cp).unwrap();
        let mut params = SelectParams::new();
        params.recipients = vec![(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())];

        let template = wallet
            .select(coins, params)
            .unwrap()
            .set_locktime(LockTime::from_consensus(new_lock_time))
            .unwrap();
        let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();
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
        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        let res = wallet.candidates_with(&cp);
        assert!(
            matches!(res, Err(CandidatesError::Plan(err)) if err == op),
            "UTXO requires CSV but the assets are insufficient",
        );
    }

    // Add assets ok
    {
        let rel_locktime = relative::LockTime::from_consensus(6).unwrap();
        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        cp.assets = Assets::new().older(rel_locktime);
        let coins = wallet.candidates_with(&cp).unwrap();
        let mut params = SelectParams::new();
        params.recipients = vec![(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())];
        let template = wallet.select(coins, params).unwrap();
        let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();
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
        let mut cp = CandidateParams::new();
        cp.must_spend = [op].into();
        let coins = wallet.candidates_with(&cp).unwrap();
        let mut params = SelectParams::new();
        params.recipients = vec![(addr.script_pubkey(), Amount::from_btc(0.42).unwrap())];
        let template = wallet.select(coins, params).unwrap();
        let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();
        assert_eq!(psbt.unsigned_tx.input[0].sequence, Sequence(6));
    }
}

/// Fallback sequence is applied to a coin-selected input that has no CSV
/// requirement.
#[test]
fn test_create_psbt_fallback_sequence_applied_to_coin_selected_input() {
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let addr = wallet.next_unused_address(KeychainKind::External);
    let coins = wallet.candidates().unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(addr.script_pubkey(), Amount::from_sat(25_000))];
    let template = wallet
        .select(coins, params)
        .unwrap()
        .set_fallback_sequence(Sequence::ENABLE_RBF_NO_LOCKTIME);
    let psbt = wallet.finish(template, FinishParams::default()).unwrap().0;
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
    let mut cp = CandidateParams::new();
    cp.must_spend = [op].into();
    cp.assets = Assets::new().older(rel_locktime);
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(addr.script_pubkey(), Amount::from_sat(25_000))];
    let template = wallet
        .select(coins, params)
        .unwrap()
        .set_fallback_sequence(Sequence::ENABLE_RBF_NO_LOCKTIME);
    let psbt = wallet.finish(template, FinishParams::default()).unwrap().0;
    // CSV descriptor requires older(6); fallback must not clobber the CSV-derived sequence.
    assert_eq!(psbt.unsigned_tx.input[0].sequence, Sequence(6));
}

/// A per-input sequence override is applied to a manually-selected UTXO.
#[test]
fn test_create_psbt_sequence_override_manually_selected_input() {
    let (mut wallet, txid) = get_funded_wallet_wpkh();
    let utxo = OutPoint::new(txid, 0);
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut cp = CandidateParams::new();
    cp.must_spend = [utxo].into();
    cp.manually_selected_only = true;
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(addr.script_pubkey(), Amount::from_sat(25_000))];
    let mut template = wallet.select(coins, params).unwrap();
    template
        .input_mut(utxo)
        .unwrap()
        .set_sequence(Sequence(42))
        .unwrap();
    let psbt = wallet.finish(template, FinishParams::default()).unwrap().0;
    assert_eq!(psbt.unsigned_tx.input[0].sequence, Sequence(42));
}

/// A per-input sequence override takes precedence over a fallback sequence.
#[test]
fn test_create_psbt_sequence_override_takes_precedence_over_fallback() {
    let (mut wallet, txid) = get_funded_wallet_wpkh();
    let utxo = OutPoint::new(txid, 0);
    let addr = wallet.next_unused_address(KeychainKind::External);
    let mut cp = CandidateParams::new();
    cp.must_spend = [utxo].into();
    cp.manually_selected_only = true;
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(addr.script_pubkey(), Amount::from_sat(25_000))];
    let mut template = wallet
        .select(coins, params)
        .unwrap()
        .set_fallback_sequence(Sequence::ENABLE_RBF_NO_LOCKTIME);
    template
        .input_mut(utxo)
        .unwrap()
        .set_sequence(Sequence(42))
        .unwrap();
    let psbt = wallet.finish(template, FinishParams::default()).unwrap().0;
    assert_eq!(psbt.unsigned_tx.input[0].sequence, Sequence(42));
}

/// Setting a template input sequence that violates the CSV requirement is rejected.
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
    let mut cp = CandidateParams::new();
    cp.must_spend = [op].into();
    cp.assets = Assets::new().older(rel_locktime);
    cp.manually_selected_only = true;
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(addr.script_pubkey(), Amount::from_sat(25_000))];
    let mut template = wallet.select(coins, params).unwrap();
    // CSV requires older(6); setting a lower sequence on the template input is rejected.
    let res = template.input_mut(op).unwrap().set_sequence(Sequence(3));
    assert!(matches!(
        res,
        Err(bdk_tx::SetSequenceError::RelativeTimelockNotSatisfied { .. })
    ));
}

// Test that replacing two unconfirmed txs A, B results in a transaction
// that spends the inputs of both A and B.
#[test]
fn test_replace_by_fee_and_recipients() {
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
    let mut cp = CandidateParams::new();
    cp.must_spend = [op0].into();
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(recip.clone(), Amount::from_sat(16_000))];
    let template = wallet.select(coins, params).unwrap();
    let txa = wallet
        .finish(template, FinishParams::default())
        .unwrap()
        .0
        .unsigned_tx;
    insert_tx(&mut wallet, txa.clone());

    // Create tx B (unconfirmed)
    let mut cp = CandidateParams::new();
    cp.must_spend = [op1].into();
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(recip.clone(), Amount::from_sat(42_000))];
    let template = wallet.select(coins, params).unwrap();
    let txb = wallet
        .finish(template, FinishParams::default())
        .unwrap()
        .0
        .unsigned_tx;
    insert_tx(&mut wallet, txb.clone());

    // Now create RBF tx
    let coins = wallet
        .rbf_candidates(&[txa.compute_txid(), txb.compute_txid()])
        .unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(recip, Amount::from_btc(1.99).unwrap())];
    params.fee_rate = FeeRate::from_sat_per_vb(4).unwrap();
    let template = wallet.select(coins, params).unwrap();
    let psbt = wallet.finish(template, FinishParams::default()).unwrap().0;

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
    let coins = wallet.rbf_candidates(&[a_txid]).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(external, Amount::from_sat(100_000))];
    params.fee_rate = FeeRate::from_sat_per_vb(4).unwrap();
    let template = wallet
        .select(coins, params)
        .expect("should select for replacement psbt");
    let (psbt, _) = wallet
        .finish(template, FinishParams::default())
        .expect("should create replacement psbt");

    let replacement_fee = wallet
        .calculate_fee(&psbt.unsigned_tx)
        .expect("replacement tx fee should be calculable");

    assert!(
        replacement_fee >= total_original_fee,
        "replacement fee ({replacement_fee}) must be >= sum of fees for A + B + C ({total_original_fee})",
    );
}

// Test that RBF rejects a confirmed original tx
#[test]
fn test_replace_by_fee_confirmed_tx_error() {
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
    let mut cp = CandidateParams::new();
    cp.must_spend = [funding_op].into();
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(recip.clone(), Amount::from_sat(100_000))];
    let template = wallet.select(coins, params).unwrap();
    let unconfirmed_tx = wallet
        .finish(template, FinishParams::default())
        .unwrap()
        .0
        .unsigned_tx;
    insert_tx(&mut wallet, unconfirmed_tx.clone());

    // Now confirm that tx.
    let confirmed_txid = unconfirmed_tx.compute_txid();
    let confirm_block = BlockId {
        height: 1001,
        hash: Hash::hash(b"1001"),
    };
    insert_tx_anchor(&mut wallet, unconfirmed_tx.clone(), confirm_block);

    // Attempting to replace the now-confirmed tx should return TransactionConfirmed.
    let result = wallet.rbf_candidates(&[confirmed_txid]);

    assert!(
        matches!(result, Err(CandidatesError::TransactionConfirmed(txid)) if txid == confirmed_txid),
        "expected TransactionConfirmed error, got: {result:?}",
    );
}

// Test that a replacement derived from the wallet graph keeps the original transaction's inputs
// as the must-spend set. (In the reshaped API the replaced txs' inputs are re-derived from the
// wallet graph, so the replacement always retains at least one input from each replaced tx.)
#[test]
fn test_replace_by_fee_keeps_original_inputs() {
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
    let mut cp = CandidateParams::new();
    cp.must_spend = [funding_op].into();
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(recip, Amount::from_sat(100_000))];
    let template = wallet.select(coins, params).unwrap();
    let unconfirmed_tx = wallet
        .finish(template, FinishParams::default())
        .unwrap()
        .0
        .unsigned_tx;
    let unconfirmed_txid = unconfirmed_tx.compute_txid();
    insert_tx(&mut wallet, unconfirmed_tx.clone());

    // The replacement set re-derives the original tx's inputs as must-spend candidates.
    let coins = wallet.rbf_candidates(&[unconfirmed_txid]).unwrap();
    assert!(coins.is_rbf());
    assert!(
        coins
            .inputs()
            .any(|input| input.prev_outpoint() == funding_op),
        "the replacement must keep the original transaction's input",
    );
}

// Test that RBF rejects a manually-selected input that spends
// from a descendant of the one being replaced.
#[test]
fn test_replace_by_fee_conflicting_input_descendant() {
    use bdk_tx::Input as BdkInput;
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
    let mut cp = CandidateParams::new();
    cp.must_spend = [funding_op].into();
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(recip.clone(), Amount::from_sat(100_000))];
    let template = wallet.select(coins, params).unwrap();
    let tx_parent = wallet
        .finish(template, FinishParams::default())
        .unwrap()
        .0
        .unsigned_tx;
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

    // Build replacement for tx_parent, then try to add the grandchild foreign input — it spends
    // an output of a replaced tx, so the push is rejected.
    let mut cp = CandidateParams::new();
    cp.replace = vec![txid_parent];
    let coins = wallet.candidates_with(&cp).unwrap();
    let result = coins.push_must_select(grandchild_input);
    assert!(
        matches!(&result, Err(CandidatesError::ConflictingInput(op)) if *op == grandchild_op),
        "expected ConflictingInput({grandchild_op}), got: {result:?}",
    );
}

// Replacing a tx whose inputs the wallet doesn't control is rejected — the wallet couldn't build a
// replacement that conflicts with (and evicts) it.
#[test]
fn test_replace_uncontrolled_tx_errors() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    // A foreign, unconfirmed tx in the graph that spends no wallet-owned output.
    let foreign_tx = Transaction {
        input: vec![TxIn {
            previous_output: OutPoint::new(Hash::hash(b"not_ours"), 0),
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(50_000),
            script_pubkey: ScriptBuf::new_p2a(),
        }],
        ..new_tx(0)
    };
    let foreign_txid = foreign_tx.compute_txid();
    insert_tx(&mut wallet, foreign_tx);

    let mut cp = CandidateParams::new();
    cp.replace = vec![foreign_txid];
    let result = wallet.candidates_with(&cp);
    assert!(
        matches!(result, Err(CandidatesError::CannotReplace(txid)) if txid == foreign_txid),
        "expected CannotReplace({foreign_txid}), got: {result:?}",
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
    assert_eq!(wallet.balance().total().to_sat(), 2100);

    let change_script = ChangeScript::from_descriptor(
        wallet
            .public_descriptor(KeychainKind::Internal)
            .at_derivation_index(0)
            .unwrap(),
    );
    // Avoid selection of dust utxos
    let coins = wallet.candidates().unwrap().filter(|input| {
        let txout = input.prev_txout();
        let min_non_dust = txout.script_pubkey.minimal_non_dust(); // 330
        txout.value >= min_non_dust
    });
    let mut params = SelectParams::new();
    params.coin_selection = psbt::SelectionStrategy::DrainAll;
    params.change_script = Some(change_script);
    params.fee_rate = FeeRate::ZERO;
    let template = wallet.select(coins, params).unwrap();
    let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();
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

    // No recipients on `select` → should error.
    let coins = wallet.candidates().unwrap();
    let err = wallet.select(coins, SelectParams::new()).unwrap_err();
    assert!(
        matches!(err, CreatePsbtError::NoRecipients),
        "expected NoRecipients, got {err:?}"
    );

    // Sending everything to a single destination is expressed via `DrainAll` with no recipients.
    let change_descriptor = wallet
        .public_descriptor(KeychainKind::Internal)
        .at_derivation_index(0)
        .unwrap();
    let coins = wallet.candidates().unwrap();
    let mut params = SelectParams::new();
    params.coin_selection = psbt::SelectionStrategy::DrainAll;
    params.change_script = Some(ChangeScript::from_descriptor(change_descriptor));
    let _template = wallet
        .select(coins, params)
        .expect("drain to an explicit destination should succeed");
}

// A drain with no explicit change script makes the wallet auto-derive and *reveal* an internal
// change address (it becomes the sole sweep destination). A drain to an explicit change script
// must not reveal anything new on the internal keychain.
#[test]
fn test_drain_reveals_auto_change() {
    // (1) Auto-change drain reveals an internal change address.
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let before = wallet.derivation_index(KeychainKind::Internal);
    let coins = wallet.candidates().unwrap();
    let mut params = SelectParams::new();
    params.coin_selection = psbt::SelectionStrategy::DrainAll;
    let _template = wallet
        .select(coins, params)
        .expect("auto-change drain should succeed");
    assert!(
        wallet.derivation_index(KeychainKind::Internal) > before,
        "auto-change drain must reveal an internal change address (before={before:?})"
    );

    // (2) Drain to an explicit change script reveals nothing new internally.
    let (mut wallet, _) = get_funded_wallet_wpkh();
    let before = wallet.derivation_index(KeychainKind::Internal);
    let change_descriptor = wallet
        .public_descriptor(KeychainKind::Internal)
        .at_derivation_index(0)
        .unwrap();
    let coins = wallet.candidates().unwrap();
    let mut params = SelectParams::new();
    params.coin_selection = psbt::SelectionStrategy::DrainAll;
    params.change_script = Some(ChangeScript::from_descriptor(change_descriptor));
    let _template = wallet
        .select(coins, params)
        .expect("explicit-destination drain should succeed");
    assert_eq!(
        wallet.derivation_index(KeychainKind::Internal),
        before,
        "drain to an explicit change script must not reveal a new internal address"
    );
}

// Manually-selected coins are de-duplicated when the `CandidateSet` is resolved.
#[test]
fn test_candidates_dedup_manual_inputs() {
    let (wallet, txid) = get_funded_wallet_wpkh();
    let op = OutPoint::new(txid, 0);

    // (1) An outpoint listed more than once in `must_spend` resolves to a single input.
    let mut cp = CandidateParams::new();
    cp.must_spend = [op, op, op].into();
    let coins = wallet.candidates_with(&cp).unwrap();
    assert_eq!(
        coins.inputs().filter(|i| i.prev_outpoint() == op).count(),
        1,
        "duplicate must-spend outpoints must resolve to a single input"
    );

    // (2) Pushing a foreign input whose outpoint is already a must-spend candidate is de-duplicated
    // (the existing candidate is kept; its contents are irrelevant here).
    let psbt_input = bitcoin::psbt::Input {
        witness_utxo: Some(TxOut {
            value: Amount::from_sat(50_000),
            script_pubkey: ScriptBuf::new_p2a(),
        }),
        ..Default::default()
    };
    let foreign = bdk_tx::Input::from_psbt_input(
        op,
        Sequence::ENABLE_LOCKTIME_NO_RBF,
        psbt_input,
        /* satisfaction_weight: */ 0,
        /* status: */ None,
        /* is_coinbase: */ false,
        /* absolute_timelock: */ None,
    )
    .unwrap();
    let mut cp = CandidateParams::new();
    cp.must_spend = [op].into();
    let coins = wallet
        .candidates_with(&cp)
        .unwrap()
        .push_must_select(foreign)
        .unwrap();
    assert_eq!(
        coins.inputs().filter(|i| i.prev_outpoint() == op).count(),
        1,
        "an outpoint that is both a must-spend and a pushed foreign input resolves once"
    );
}

// Draining a wallet with no spendable value cannot even cover fees, so selection fails rather than
// producing an empty/invalid transaction.
#[test]
fn test_drain_empty_wallet_errors() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    let coins = wallet.candidates().unwrap();
    assert!(coins.is_empty(), "fresh wallet has no candidates");

    let mut params = SelectParams::new();
    params.coin_selection = psbt::SelectionStrategy::DrainAll;
    let err = wallet.select(coins, params).unwrap_err();
    assert!(
        matches!(err, CreatePsbtError::Selector(_)),
        "empty-wallet drain should fail to meet target, got {err:?}"
    );
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

// When a sweep's only output would fall below the dust threshold, verify that `sweep`
// surfaces this as an error rather than returning a zero-output PSBT.
#[test]
fn test_sweep_change_below_dust_error() {
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
    let coins = wallet.candidates().unwrap();
    let mut params = SelectParams::new();
    params.coin_selection = psbt::SelectionStrategy::DrainAll;
    params.change_script = Some(ChangeScript::from_descriptor(change_descriptor));

    let err = wallet.select(coins, params).unwrap_err();
    assert!(
        matches!(err, CreatePsbtError::AllOutputsBelowDust),
        "expected AllOutputsBelowDust when swept output is below dust threshold, got {err:?}"
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

    let mut cp = CandidateParams::new();
    cp.must_spend = [funding_op].into();
    let coins = wallet
        .candidates_with(&cp)
        .unwrap()
        .push_must_select(planned_input.clone())
        .unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(recip.clone(), Amount::from_sat(100_000))];
    let template = wallet.select(coins, params).unwrap();
    let unconfirmed_tx = wallet
        .finish(template, FinishParams::default())
        .unwrap()
        .0
        .unsigned_tx;
    let unconfirmed_txid = unconfirmed_tx.compute_txid();
    insert_tx(&mut wallet, unconfirmed_tx.clone());

    // Add the foreign input alongside the replacement. The pushed foreign input must be respected
    // (and de-duplicated) in the replacement's candidate set.
    let mut cp = CandidateParams::new();
    cp.replace = vec![unconfirmed_txid];
    let coins = wallet
        .candidates_with(&cp)
        .unwrap()
        .push_must_select(planned_input.clone())
        .unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(recip, Amount::from_sat(99_000))];

    let template = wallet
        .select(coins, params)
        .expect("replacement should succeed");
    let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();
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

// Test that a Replace-By-Fee candidate set can be fed to `sweep` (now newly possible):
// create + broadcast an unconfirmed tx, then sweep-replace it at a higher feerate and
// assert the replacement spends the same input.
#[test]
fn test_sweep_replace_by_fee() {
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
            value: Amount::from_sat(1_000_000),
            script_pubkey: addr.script_pubkey(),
        }],
        ..new_tx(0)
    };
    let funding_op = OutPoint::new(funding_tx.compute_txid(), 0);
    insert_tx_anchor(&mut wallet, funding_tx, block);

    // Create + "broadcast" (insert) an unconfirmed tx paying an external recipient at a low
    // feerate.
    let recip =
        ScriptBuf::from_hex("5120e8f5c4dc2f5d6a7595e7b108cb063da9c7550312da1e22875d78b9db62b59cd5")
            .unwrap();
    let mut cp = CandidateParams::new();
    cp.must_spend = [funding_op].into();
    let coins = wallet.candidates_with(&cp).unwrap();
    let mut params = SelectParams::new();
    params.recipients = vec![(recip.clone(), Amount::from_sat(100_000))];
    params.fee_rate = FeeRate::from_sat_per_vb(1).unwrap();
    let template = wallet.select(coins, params).unwrap();
    let original_tx = wallet
        .finish(template, FinishParams::default())
        .unwrap()
        .0
        .unsigned_tx;
    let original_txid = original_tx.compute_txid();
    insert_tx(&mut wallet, original_tx.clone());

    // Sweep-replace the original tx at a higher feerate, draining everything to a single
    // destination.
    let dest = ChangeScript::from_descriptor(
        wallet
            .public_descriptor(Internal)
            .at_derivation_index(0)
            .unwrap(),
    );
    let coins = wallet.rbf_candidates(&[original_txid]).unwrap();
    assert!(coins.is_rbf(), "candidate set should carry RBF context");
    let mut sweep_params = SelectParams::new();
    sweep_params.coin_selection = psbt::SelectionStrategy::DrainAll;
    sweep_params.change_script = Some(dest);
    sweep_params.fee_rate = FeeRate::from_sat_per_vb(10).unwrap();
    let template = wallet
        .select(coins, sweep_params)
        .expect("sweep replacement should succeed");
    let (psbt, _) = wallet.finish(template, FinishParams::default()).unwrap();

    // The replacement must spend the same input as the original tx.
    assert!(
        psbt.unsigned_tx
            .input
            .iter()
            .any(|txin| txin.previous_output == funding_op),
        "sweep replacement must spend the original input"
    );
}

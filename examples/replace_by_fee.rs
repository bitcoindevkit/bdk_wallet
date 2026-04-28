#![allow(clippy::print_stdout)]

use std::sync::Arc;

use bdk_chain::BlockId;
use bdk_tx::ChangeScript;
use bdk_wallet::psbt::PsbtParams;
use bdk_wallet::test_utils::*;
use bdk_wallet::{KeychainKind, Wallet};
use bitcoin::{Amount, FeeRate, TxIn, TxOut};
use miniscript::{DefiniteDescriptorKey, Descriptor};

// This example demonstrates creating a sweep transaction using PsbtParams and replacing it with a
// higher feerate.

const NETWORK: bitcoin::Network = bitcoin::Network::Regtest;

fn main() -> anyhow::Result<()> {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();

    // Create wallet and "fund" it with a single UTXO.
    let mut wallet = Wallet::create(desc, change_desc)
        .network(NETWORK)
        .create_wallet_no_persist()?;

    fund_wallet(&mut wallet)?;

    // Create PSBT Signer, external to the wallet
    let signer = {
        let secp = wallet.secp_ctx();
        let (_, external_keymap) = miniscript::Descriptor::parse_descriptor(secp, desc)?;
        let (_, internal_keymap) = miniscript::Descriptor::parse_descriptor(secp, change_desc)?;
        bdk_tx::Signer(external_keymap.into_iter().chain(internal_keymap).collect())
    };

    // Get a derived descriptor from the wallet to sweep funds to
    let derived_descriptor: Descriptor<DefiniteDescriptorKey> = wallet
        .public_descriptor(KeychainKind::External)
        .at_derivation_index(1)?;

    println!(
        "Wallet funded with {}\n",
        wallet.balance().total().display_dynamic()
    );
    println!("Creating first sweep transaction (tx1)...");

    // Create tx1: sweep all funds to our own address at a low feerate
    let mut params = PsbtParams::new();
    params
        .drain_wallet()
        .change_script(ChangeScript::from_descriptor(derived_descriptor.clone()))
        .fee_rate(FeeRate::from_sat_per_vb(2).expect("valid feerate"));

    let (mut psbt1, finalizer1) = wallet.create_psbt(params)?;

    // Sign and finalize tx1
    let _ = psbt1
        .sign(&signer, wallet.secp_ctx())
        .map_err(|(_, errors)| anyhow::anyhow!("failed to sign PSBT: {errors:?}"))?;
    println!("tx1 signed: {}", !psbt1.inputs[0].partial_sigs.is_empty());

    let finalize_res = finalizer1.finalize(&mut psbt1);
    println!("tx1 finalized: {}", finalize_res.is_finalized());

    let tx1 = Arc::new(psbt1.extract_tx()?);
    let feerate1 = wallet.calculate_fee_rate(&tx1)?;
    let fee1 = wallet.calculate_fee(&tx1)?;

    println!("  txid: {}", tx1.compute_txid());
    println!(
        "  fee rate: {} sat/vb",
        bdk_wallet::floating_rate!(feerate1)
    );
    println!("  absolute fee: {} sats", fee1.to_sat());

    // Apply tx1 to wallet as unconfirmed
    wallet.apply_unconfirmed_txs([(tx1.clone(), 1234567000)]);

    println!("\nCreating RBF replacement transaction (tx2)...");

    // Create tx2: Replace tx1 at a higher feerate using PsbtParams
    let mut rbf_params = PsbtParams::new().replace_txs(&[Arc::clone(&tx1)]);

    // Set higher feerate for the replacement
    rbf_params.fee_rate(FeeRate::from_sat_per_vb(5).expect("valid feerate"));

    // Retain the original sweep destination
    rbf_params.change_script(ChangeScript::from_descriptor(derived_descriptor));

    let (mut psbt2, finalizer2) = wallet.replace_by_fee(rbf_params)?;

    // Sign and finalize tx2
    let _ = psbt2
        .sign(&signer, wallet.secp_ctx())
        .map_err(|(_, errors)| anyhow::anyhow!("failed to sign PSBT: {errors:?}"))?;
    println!("tx2 signed: {}", !psbt2.inputs[0].partial_sigs.is_empty());

    let finalize_res = finalizer2.finalize(&mut psbt2);
    println!("tx2 finalized: {}", finalize_res.is_finalized());

    let tx2 = psbt2.extract_tx()?;
    let feerate2 = wallet.calculate_fee_rate(&tx2)?;
    let fee2 = wallet.calculate_fee(&tx2)?;

    println!("  txid: {}", tx2.compute_txid());
    println!(
        "  fee rate: {} sat/vb",
        bdk_wallet::floating_rate!(feerate2)
    );
    println!("  absolute fee: {} sats", fee2.to_sat());

    println!("\nVerifying RBF properties...");

    // Verify that tx1 and tx2 conflict (spend the same input)
    let tx1_input = tx1.input[0].previous_output;
    let tx2_input = tx2.input[0].previous_output;

    assert_eq!(
        tx1_input, tx2_input,
        "ERROR: tx1 and tx2 must spend the same input"
    );
    println!("✓ Both transactions spend the same input: {}", tx1_input);

    // Verify that tx2 has a higher feerate than tx1
    assert!(
        feerate2 > feerate1,
        "ERROR: tx2 must have a higher feerate than tx1"
    );
    println!(
        "✓ Replacement has higher fee rate ({} vs {} sat/vb)",
        bdk_wallet::floating_rate!(feerate2),
        bdk_wallet::floating_rate!(feerate1)
    );

    // Verify absolute fee increase
    assert!(fee2 > fee1, "ERROR: tx2 must have a higher fee than tx1");
    let fee_increase = fee2.to_sat() as i64 - fee1.to_sat() as i64;
    println!("✓ Absolute fee increased by {} sats", fee_increase);

    // Apply tx2 to wallet so it recognizes the conflict
    wallet.apply_unconfirmed_txs([(tx2.clone(), 1234567001)]);

    // Verify that the wallet recognizes the conflict
    let txid2 = tx2.compute_txid();
    assert!(
        wallet
            .tx_graph()
            .direct_conflicts(&tx1)
            .any(|(_, txid)| txid == txid2),
        "ERROR: Wallet does not recognize tx2 as replacing tx1",
    );
    println!("✓ Wallet recognizes the transaction conflict");

    println!("\n✓✓✓ RBF sweep complete! ✓✓✓");

    Ok(())
}

fn fund_wallet(wallet: &mut Wallet) -> anyhow::Result<()> {
    let anchor_block = BlockId {
        height: 1,
        hash: "3bcc1c447c6b3886f43e416b5c21cf5c139dc4829a71dc78609bc8f6235611c5".parse()?,
    };
    let chain_tip = BlockId {
        height: 101,
        hash: "7f96292d115d19450e4bf7d4c4e15c9f3ad21e3a3cf616c498110b988963470b".parse()?,
    };

    insert_checkpoint(wallet, anchor_block);

    let addr = wallet.reveal_next_address(KeychainKind::External).address;
    let tx = bitcoin::Transaction {
        lock_time: bitcoin::absolute::LockTime::ZERO,
        version: bitcoin::transaction::Version::TWO,
        input: vec![TxIn::default()],
        output: vec![TxOut {
            script_pubkey: addr.script_pubkey(),
            value: Amount::from_sat(50_000_000),
        }],
    };
    insert_tx_anchor(wallet, tx, anchor_block);
    insert_checkpoint(wallet, chain_tip);

    Ok(())
}

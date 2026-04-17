#![allow(clippy::print_stdout)]

use std::str::FromStr;
use std::sync::Arc;

use bdk_chain::BlockId;
use bdk_tx::ScriptSource;
use bdk_wallet::psbt::PsbtParams;
use bdk_wallet::test_utils::*;
use bdk_wallet::{KeychainKind, Wallet};
use bitcoin::{bip32, consensus, secp256k1, FeeRate, Transaction};
use miniscript::{DefiniteDescriptorKey, Descriptor};

// This example demonstrates creating a sweep transaction using PsbtParams and replacing it with a
// higher feerate.

const NETWORK: bitcoin::Network = bitcoin::Network::Regtest;
const XPRIV: &str = "tprv8ZgxMBicQKsPe5tkv8BYJRupCNULhJYDv6qrtVAK9fNVheU6TbscSedVi8KQk8vVZqXMnsGomtVkR4nprbgsxTS5mAQPV4dpPXNvsmYcgZU";

fn main() -> anyhow::Result<()> {
    let desc = "wpkh([7a5a223e/84'/1'/0']tpubDCpz3tR7UiAy1crSewah3t4kYgcSoBS2bJhGpK8VxrMnv8Ecbmw31DvYwhcsouVpETr8t2NinEyryMQtXbw1ujpQLu6WjHGnhqZRi7tV7pi/0/*)#ls3ewa0d";
    let change_desc = "wpkh([7a5a223e/84'/1'/0']tpubDCpz3tR7UiAy1crSewah3t4kYgcSoBS2bJhGpK8VxrMnv8Ecbmw31DvYwhcsouVpETr8t2NinEyryMQtXbw1ujpQLu6WjHGnhqZRi7tV7pi/1/*)#wy5cngl4";
    let secp = secp256k1::Secp256k1::new();

    // Xpriv to be used for signing the PSBT
    let xprv = bip32::Xpriv::from_str(XPRIV)?;

    // Create wallet and "fund" it with a single UTXO.
    let mut wallet = Wallet::create(desc, change_desc)
        .network(NETWORK)
        .create_wallet_no_persist()?;

    let _funding_tx = fund_wallet(&mut wallet)?;

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
        .change_script(ScriptSource::from(derived_descriptor.clone()))
        .fee_rate(FeeRate::from_sat_per_vb_unchecked(2));

    let (mut psbt1, finalizer1) = wallet.create_psbt(params)?;

    // Sign and finalize tx1
    let _ = psbt1.sign(&xprv, &secp);
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
    rbf_params.fee_rate(FeeRate::from_sat_per_vb_unchecked(5));

    // Retain the original sweep destination
    rbf_params.change_script(ScriptSource::from(derived_descriptor));

    let (mut psbt2, finalizer2) = wallet.replace_by_fee(rbf_params)?;

    // Sign and finalize tx2
    let _ = psbt2.sign(&xprv, &secp);
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

fn fund_wallet(wallet: &mut Wallet) -> anyhow::Result<Arc<Transaction>> {
    // First, we need a confirmed coinbase transaction
    let coinbase_tx: Transaction = consensus::encode::deserialize_hex(
        "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff025100ffffffff0200f2052a010000001600144d34238b9c4c59b9e2781e2426a142a75b8901ab0000000000000000266a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf90120000000000000000000000000000000000000000000000000000000000000000000000000",
    )?;

    let anchor_block = BlockId {
        height: 1,
        hash: "3bcc1c447c6b3886f43e416b5c21cf5c139dc4829a71dc78609bc8f6235611c5".parse()?,
    };
    let chain_tip = BlockId {
        height: anchor_block.height + bitcoin::constants::COINBASE_MATURITY,
        hash: "7f96292d115d19450e4bf7d4c4e15c9f3ad21e3a3cf616c498110b988963470b".parse()?,
    };

    insert_tx_anchor(wallet, coinbase_tx.clone(), anchor_block);
    insert_checkpoint(wallet, chain_tip);

    Ok(Arc::new(coinbase_tx))
}

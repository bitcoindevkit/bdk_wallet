#![allow(clippy::print_stdout)]

use std::collections::HashMap;
use std::str::FromStr;

use bdk_chain::BlockId;
use bdk_chain::ConfirmationBlockTime;
use bdk_wallet::psbt::{PsbtParams, SelectionStrategy::*};
use bdk_wallet::test_utils::*;
use bdk_wallet::{KeychainKind::External, Wallet};
use bitcoin::{
    bip32, consensus,
    secp256k1::{self, rand},
    Address, Amount, TxIn, TxOut,
};
use rand::Rng;

// This example shows how to create a PSBT using BDK Wallet.

const NETWORK: bitcoin::Network = bitcoin::Network::Signet;
const SEND_TO: &str = "tb1pw3g5qvnkryghme7pyal228ekj6vq48zc5k983lqtlr2a96n4xw0q5ejknw";
const AMOUNT: Amount = Amount::from_sat(42_000);
const FEERATE: f64 = 2.0; // sat/vb

fn main() -> anyhow::Result<()> {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let secp = secp256k1::Secp256k1::new();

    // Xpriv to be used for signing the PSBT
    let xprv = bip32::Xpriv::from_str("tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L")?;

    // Create wallet and fund it.
    let mut wallet = Wallet::create(desc, change_desc)
        .network(NETWORK)
        .create_wallet_no_persist()?;

    fund_wallet(&mut wallet)?;

    let utxos = wallet
        .list_unspent()
        .map(|output| (output.outpoint, output))
        .collect::<HashMap<_, _>>();

    // Build params.
    let mut params = PsbtParams::default();
    let addr = Address::from_str(SEND_TO)?.require_network(NETWORK)?;
    let feerate = feerate_unchecked(FEERATE);
    params
        .add_recipients([(addr, AMOUNT)])
        .fee(bdk_tx::FeeStrategy::FeeRate(feerate))
        .coin_selection(SingleRandomDraw);

    // Create PSBT (which also returns the Finalizer).
    let (mut psbt, finalizer) = wallet.create_psbt(params)?;

    dbg!(&psbt);

    let tx = &psbt.unsigned_tx;
    for txin in &tx.input {
        let op = txin.previous_output;
        let output = utxos.get(&op).unwrap();
        println!("TxIn: {}", output.txout.value);
    }
    for txout in &tx.output {
        println!("TxOut: {}", txout.value);
    }

    let _ = psbt.sign(&xprv, &secp);
    println!("Signed: {}", !psbt.inputs[0].partial_sigs.is_empty());
    let finalize_res = finalizer.finalize(&mut psbt);
    println!("Finalized: {}", finalize_res.is_finalized());

    let tx = psbt.extract_tx()?;
    let feerate = wallet.calculate_fee_rate(&tx)?;
    println!("Fee rate: {} sat/vb", bdk_wallet::floating_rate!(feerate));

    println!("{}", consensus::encode::serialize_hex(&tx));

    Ok(())
}

fn fund_wallet(wallet: &mut Wallet) -> anyhow::Result<()> {
    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 260071,
            hash: "000000099f67ae6469d1ad0525d756e24d4b02fbf27d65b3f413d5feb367ec48".parse()?,
        },
        confirmation_time: 1752184658,
    };
    insert_checkpoint(wallet, anchor.block_id);

    let mut rng = rand::thread_rng();

    // Fund wallet with several random utxos
    for i in 0..21 {
        let addr = wallet.reveal_next_address(External).address;
        let value = 10_000 * (i + 1) + (100 * rng.gen_range(0..10));
        let tx = bitcoin::Transaction {
            lock_time: bitcoin::absolute::LockTime::ZERO,
            version: bitcoin::transaction::Version::TWO,
            input: vec![TxIn::default()],
            output: vec![TxOut {
                script_pubkey: addr.script_pubkey(),
                value: Amount::from_sat(value),
            }],
        };
        insert_tx_anchor(wallet, tx, anchor.block_id);
    }

    let tip = BlockId {
        height: 260171,
        hash: "0000000b9efb77450e753ae9fd7be9f69219511c27b6e95c28f4126f3e1591c3".parse()?,
    };
    insert_checkpoint(wallet, tip);

    Ok(())
}

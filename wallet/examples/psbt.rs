#![allow(unused_imports)]
#![allow(clippy::print_stdout)]

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use bdk_chain::BlockId;
use bdk_chain::ConfirmationBlockTime;
use bdk_chain::TxUpdate;
use bdk_wallet::psbt::params::{Params, SelectionStrategy::*};
use bdk_wallet::test_utils::*;
use bdk_wallet::{KeychainKind::*, Update, Wallet};
use bitcoin::FeeRate;
use bitcoin::{
    bip32, consensus,
    secp256k1::{self, rand},
    Address, Amount, OutPoint, TxIn, TxOut,
};
use miniscript::descriptor::Descriptor;
use miniscript::descriptor::KeyMap;
use rand::Rng;

// This example shows how to create a PSBT using BDK Wallet.

const NETWORK: bitcoin::Network = bitcoin::Network::Signet;
const SEND_TO: &str = "tb1pw3g5qvnkryghme7pyal228ekj6vq48zc5k983lqtlr2a96n4xw0q5ejknw";
const AMOUNT: Amount = Amount::from_sat(42_000);
const FEERATE: f64 = 2.0; // sat/vb

fn main() -> anyhow::Result<()> {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let secp = secp256k1::Secp256k1::new();
    let mut rng = rand::thread_rng();

    // Xpriv to be used for signing the PSBT
    let xprv = bip32::Xpriv::from_str("tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L")?;

    // Create wallet and fund it.
    let mut wallet = Wallet::create(desc, change_desc)
        .network(NETWORK)
        .create_wallet_no_persist()?;

    fund_wallet(&mut wallet, &mut rng)?;

    let utxos = wallet
        .list_unspent()
        .map(|output| (output.outpoint, output))
        .collect::<HashMap<_, _>>();

    // Build params.
    let mut params = Params::default();
    let addr = Address::from_str(SEND_TO)?.require_network(NETWORK)?;
    let feerate = feerate_unchecked(FEERATE);
    params
        .add_recipients([(addr, AMOUNT)])
        .feerate(feerate)
        .coin_selection(SingleRandomDraw);

    // Create PSBT (which also returns the Finalizer).
    let (mut psbt, finalizer) = wallet.create_psbt(params, &mut rng)?;

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

    let sign_res = psbt.sign(&xprv, &secp);
    println!("Signed: {}", sign_res.is_ok());

    let finalize_res = finalizer.finalize(&mut psbt);
    println!("Finalized: {}", finalize_res.is_finalized());

    let tx = psbt.extract_tx()?;

    let feerate = wallet.calculate_fee_rate(&tx)?;
    println!("Feerate: {} sat/vb", bdk_wallet::floating_rate!(feerate));

    println!("{}", consensus::encode::serialize_hex(&tx));

    Ok(())
}

fn fund_wallet(wallet: &mut Wallet, rng: &mut impl Rng) -> anyhow::Result<()> {
    let anchor = ConfirmationBlockTime {
        block_id: BlockId {
            height: 260071,
            hash: "000000099f67ae6469d1ad0525d756e24d4b02fbf27d65b3f413d5feb367ec48".parse()?,
        },
        confirmation_time: 1752184658,
    };
    insert_checkpoint(wallet, anchor.block_id);

    // Fund wallet with several random utxos
    for i in 0..21 {
        let value = 10_000 * (i + 1) + (100 * rng.gen_range(0..10));
        let addr = wallet.reveal_next_address(External).address;
        receive_output_to_addr(
            wallet,
            addr,
            Amount::from_sat(value),
            ReceiveTo::Block(anchor),
        );
    }

    Ok(())
}

// Note: this is borrowed from `test-utils`, but here the tx appears as a coinbase tx
// and inserting it does not automatically include a timestamp.
fn receive_output_to_addr(
    wallet: &mut Wallet,
    addr: Address,
    value: Amount,
    receive_to: impl Into<ReceiveTo>,
) -> OutPoint {
    let tx = bitcoin::Transaction {
        lock_time: bitcoin::absolute::LockTime::ZERO,
        version: bitcoin::transaction::Version::TWO,
        input: vec![TxIn::default()],
        output: vec![TxOut {
            script_pubkey: addr.script_pubkey(),
            value,
        }],
    };

    // Insert tx
    let txid = tx.compute_txid();
    let mut tx_update = TxUpdate::default();
    tx_update.txs = vec![Arc::new(tx)];
    wallet
        .apply_update(Update {
            tx_update,
            ..Default::default()
        })
        .unwrap();

    // Insert anchor or last-seen.
    match receive_to.into() {
        ReceiveTo::Block(anchor) => insert_anchor(wallet, txid, anchor),
        ReceiveTo::Mempool(last_seen) => insert_seen_at(wallet, txid, last_seen),
    }

    OutPoint { txid, vout: 0 }
}

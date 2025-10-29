#![allow(clippy::print_stdout)]

use std::str::FromStr;
use std::sync::Arc;

use bdk_chain::BlockId;
use bdk_wallet::test_utils::*;
use bdk_wallet::Wallet;
use bitcoin::{bip32, consensus, secp256k1, Address, FeeRate, Transaction};

// This example shows how to create a Replace-By-Fee (RBF) transaction using BDK Wallet.

const NETWORK: bitcoin::Network = bitcoin::Network::Regtest;
const SEND_TO: &str = "bcrt1q3yfqg2v9d605r45y5ddt5unz5n8v7jl5yk4a4f";

fn main() -> anyhow::Result<()> {
    let desc = "wpkh(tprv8ZgxMBicQKsPe5tkv8BYJRupCNULhJYDv6qrtVAK9fNVheU6TbscSedVi8KQk8vVZqXMnsGomtVkR4nprbgsxTS5mAQPV4dpPXNvsmYcgZU/84h/1h/0h/0/*)";
    let change_desc = "wpkh(tprv8ZgxMBicQKsPe5tkv8BYJRupCNULhJYDv6qrtVAK9fNVheU6TbscSedVi8KQk8vVZqXMnsGomtVkR4nprbgsxTS5mAQPV4dpPXNvsmYcgZU/84h/1h/0h/1/*)";
    let secp = secp256k1::Secp256k1::new();

    // Xpriv to be used for signing the PSBT
    let xprv = bip32::Xpriv::from_str("tprv8ZgxMBicQKsPe5tkv8BYJRupCNULhJYDv6qrtVAK9fNVheU6TbscSedVi8KQk8vVZqXMnsGomtVkR4nprbgsxTS5mAQPV4dpPXNvsmYcgZU")?;

    // Create wallet and "fund" it.
    let mut wallet = Wallet::create(desc, change_desc)
        .network(NETWORK)
        .create_wallet_no_persist()?;

    // `tx_1` is the unconfirmed wallet tx that we want to replace.
    let tx_1 = fund_wallet(&mut wallet)?;
    wallet.apply_unconfirmed_txs([(tx_1.clone(), 1234567000)]);

    // We'll need to fill in the original recipient details.
    let addr = Address::from_str(SEND_TO)?.require_network(NETWORK)?;
    let txo = tx_1
        .output
        .iter()
        .find(|txo| txo.script_pubkey == addr.script_pubkey())
        .expect("failed to find orginal recipient")
        .clone();

    // Now build fee bump.
    let (mut psbt, finalizer) = wallet.replace_by_fee_and_recipients(
        &[Arc::clone(&tx_1)],
        FeeRate::from_sat_per_vb_unchecked(5),
        vec![(txo.script_pubkey, txo.value)],
    )?;

    let _ = psbt.sign(&xprv, &secp);
    println!("Signed: {}", !psbt.inputs[0].partial_sigs.is_empty());
    let finalize_res = finalizer.finalize(&mut psbt);
    println!("Finalized: {}", finalize_res.is_finalized());

    let tx = psbt.extract_tx()?;
    let feerate = wallet.calculate_fee_rate(&tx)?;
    println!("Fee rate: {} sat/vb", bdk_wallet::floating_rate!(feerate));

    println!("{}", consensus::encode::serialize_hex(&tx));

    wallet.apply_unconfirmed_txs([(tx.clone(), 1234567001)]);

    let txid_2 = tx.compute_txid();

    assert!(
        wallet
            .tx_graph()
            .direct_conflicts(&tx_1)
            .any(|(_, txid)| txid == txid_2),
        "ERROR: RBF tx does not replace `tx_1`",
    );

    Ok(())
}

fn fund_wallet(wallet: &mut Wallet) -> anyhow::Result<Arc<Transaction>> {
    // The parent of `tx`. This is needed to compute the original fee.
    let tx0: Transaction = consensus::encode::deserialize_hex(
        "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff025100ffffffff0200f2052a010000001600144d34238b9c4c59b9e2781e2426a142a75b8901ab0000000000000000266a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf90120000000000000000000000000000000000000000000000000000000000000000000000000",
    )?;

    let anchor_block = BlockId {
        height: 101,
        hash: "3bcc1c447c6b3886f43e416b5c21cf5c139dc4829a71dc78609bc8f6235611c5".parse()?,
    };
    insert_tx_anchor(wallet, tx0, anchor_block);

    let tx: Transaction = consensus::encode::deserialize_hex(
        "020000000001014cb96536e94ba3f840cb5c2c965c8f9a306209de63fcd02060219aaf14f1d7b30000000000fdffffff0280de80020000000016001489120429856e9f41d684a35aba7262a4cecf4bf4f312852701000000160014757a57b3009c0e9b2b9aa548434dc295e21aeb05024730440220400c0a767ce42e0ea02b72faabb7f3433e607b475111285e0975bba1e6fd2e13022059453d83cbacb6652ba075f59ca0437036f3f94cae1959c7c5c0f96a8954707a012102c0851c2d2bddc1dd0b05caeac307703ec0c4b96ecad5a85af47f6420e2ef6c661b000000",
    )?;

    Ok(Arc::new(tx))
}

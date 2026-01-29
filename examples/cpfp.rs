use bdk_tx::Signer;
use bdk_wallet::{KeychainKind, Wallet};
use bitcoin::{
    consensus::encode::deserialize_hex, secp256k1::Secp256k1, Amount, Network, OutPoint, ScriptBuf,
    Transaction, TxOut,
};
use miniscript::Descriptor;
use std::sync::Arc;

const EXTERNAL: &str = "tr(tprv8ZgxMBicQKsPd3krDUsBAmtnRsK3rb8u5yi1zhQgMhF1tR8MW7xfE4rnrbbsrbPR52e7rKapu6ztw1jXveJSCGHEriUGZV7mCe88duLp5pj/86'/1'/0'/0/*)";
const INTERNAL: &str = "tr(tprv8ZgxMBicQKsPd3krDUsBAmtnRsK3rb8u5yi1zhQgMhF1tR8MW7xfE4rnrbbsrbPR52e7rKapu6ztw1jXveJSCGHEriUGZV7mCe88duLp5pj/86'/1'/0'/1/*)";

const FEERATE: f32 = 10.0;

fn main() -> anyhow::Result<()> {
    let secp = Secp256k1::new();
    let (external_desc, mut keymap) = Descriptor::parse_descriptor(&secp, EXTERNAL)?;
    let (internal_desc, internal_keymap) = Descriptor::parse_descriptor(&secp, INTERNAL)?;
    keymap.extend(internal_keymap);

    let mut wallet = Wallet::create(external_desc, internal_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()?;

    // Track balances for sanity checking
    let initial_balance = wallet.balance().total();
    println!("Initial balance: {}", initial_balance);

    let tx0: Transaction = deserialize_hex(
        "02000000000101401087cb611c1173462be69d8abb501edaf0e89cf086d0c88e377043fc7f6bde0000000000fdffffff02db1285270100000022512049c3c5eac192a9ee551f1a3a45bbb47c68c7c01e8d007847a44cdca20080a55f80de80020000000022512005472086085253543288c12a67aa2975f1e8e698b1f026d625238ef84abbfe2b024730440220787949255eb0af8e9f69b6e4f112a3a157c02a4498b87f5dede45eafd46405390220435c5562e86d1a2ad3f752d90d1fb877d8a207b09b5688c5d3371c201c534f9e012102cb066247461fb43246467b94f72497be4f5fa863baeca191c431648559e7efd365000000",
    )?;
    let tx0 = Arc::new(tx0);
    let outpoint = fund_wallet(&mut wallet, tx0.clone())?;

    let funded_balance = wallet.balance().total();
    println!("Balance after funding: {} sat", funded_balance);

    let next_index = wallet.next_derivation_index(KeychainKind::External);
    let definite_descriptor: Descriptor<miniscript::DefiniteDescriptorKey> = wallet
        .public_descriptor(KeychainKind::External)
        .at_derivation_index(next_index)?;

    let target_feerate = bdk_coin_select::FeeRate::from_sat_per_vb(FEERATE);

    let (mut psbt, finalizer) =
        wallet.create_sweep(outpoint, definite_descriptor, target_feerate)?;

    let _ = psbt.sign(&Signer(keymap), &secp).unwrap();
    let res = finalizer.finalize(&mut psbt);
    assert!(res.is_finalized());

    let tx1 = psbt.extract_tx().expect("Must be finalized!");
    assert_eq!(tx1.input.len(), 1, "Child should have 1 input");
    assert_eq!(
        tx1.input[0].previous_output, outpoint,
        "Should spend from parent"
    );

    wallet.apply_unconfirmed_txs([(Arc::new(tx1.clone()), 110)]);
    let tx1 = Arc::new(tx1);

    compute_feerate(&wallet, &[tx0, tx1]);

    let final_balance = wallet.balance().total();
    println!("Final balance: {} sat", final_balance);

    Ok(())
}

/// Compute the package feerate and print it to stdout.
fn compute_feerate(wallet: &Wallet, txs: &[Arc<Transaction>]) {
    let mut acc_fee = 0;
    let mut acc_vsize = 0;

    for tx in txs {
        let fee = wallet.calculate_fee(tx).unwrap().to_sat();
        let vsize = tx.vsize() as u64;
        let feerate = fee as f32 / vsize as f32;
        println!("Fee {fee} Vsize {vsize} FeeRate {}", feerate);
        acc_fee += fee;
        acc_vsize += vsize;
    }

    println!("Target feerate {}", FEERATE);
    println!("Package feerate {}", acc_fee as f32 / acc_vsize as f32);
}

fn fund_wallet(wallet: &mut Wallet, tx0: Arc<Transaction>) -> anyhow::Result<OutPoint> {
    let txid0 = tx0.compute_txid();

    // Previous output of tx0. This is needed for fee calculation.
    let prevout = OutPoint::new(
        "de6b7ffc4370378ec8d086f09ce8f0da1e50bb8a9de62b4673111c61cb871040".parse()?,
        0,
    );
    let txout = TxOut {
        script_pubkey: ScriptBuf::from_hex("0014ca5688311d4d0637f1c66bfd495eee02c5fe1755")?,
        value: Amount::from_btc(50.0)?,
    };
    wallet.insert_txout(prevout, txout);
    wallet.apply_unconfirmed_txs([(tx0.clone(), 100)]);

    let outpoint = tx0
        .output
        .iter()
        .enumerate()
        .find(|(_vout, txo)| txo.value == Amount::from_btc(0.42).unwrap())
        .map(|(vout, _)| OutPoint::new(txid0, vout as u32))
        .unwrap();

    Ok(outpoint)
}

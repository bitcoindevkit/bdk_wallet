use bdk_wallet::{
    bitcoin::{
        consensus::encode::serialize_hex, Amount, FeeRate, OutPoint, Sequence, Transaction, Weight,
    },
    test_utils::get_funded_wallet,
    KeychainKind, SignOptions, Wallet,
};

const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";

const PARENT_FEE: Amount = Amount::from_sat(200);

/// Demonstrates child-pays-for-parent (CPFP) via `TxBuilder`.
///
/// The example estimates the child weight, then sets the child's absolute fee so the
/// parent+child package meets the target rate.
fn main() -> anyhow::Result<()> {
    let target_package_feerate = FeeRate::from_sat_per_vb_u32(10);
    let (mut wallet, funding_txid) = get_funded_wallet(EXTERNAL_DESC, INTERNAL_DESC);
    let funding_outpoint = OutPoint::new(funding_txid, 0);

    let (parent_tx, parent_outpoint) = create_parent(&mut wallet, funding_outpoint)?;
    let child_drain_script = wallet
        .reveal_next_address(KeychainKind::External)
        .script_pubkey();

    // Build unsigned child PSBT used to estimate the weight.
    let mut probe_builder = wallet.build_tx();
    probe_builder
        .add_utxo(parent_outpoint)?
        .manually_selected_only()
        .drain_to(child_drain_script.clone())
        .fee_rate(FeeRate::BROADCAST_MIN);
    let weight_probe_child_psbt = probe_builder.finish()?;

    let estimated_child_weight = weight_probe_child_psbt.unsigned_tx.weight()
        // segwit overhead (empty-witness tx serializes as legacy). marker + flag + input witness count varint.
        + Weight::from_wu(3)
        + wallet
            .public_descriptor(KeychainKind::External)
            .max_weight_to_satisfy()?;

    let parent_fee = wallet.calculate_fee(&parent_tx)?;
    let parent_weight = parent_tx.weight();
    let required_child_fee = required_child_fee_to_meet_target(
        parent_fee,
        parent_weight,
        estimated_child_weight,
        target_package_feerate,
    );

    // Build the child sweep.
    let mut child_builder = wallet.build_tx();
    child_builder
        .add_utxo(parent_outpoint)?
        .manually_selected_only()
        .drain_to(child_drain_script)
        .fee_absolute(required_child_fee);
    let mut child_psbt = child_builder.finish()?;
    wallet.sign(&mut child_psbt, SignOptions::default())?;

    let child_tx = child_psbt.extract_tx()?;
    let child_fee = wallet.calculate_fee(&child_tx)?;
    let child_weight = child_tx.weight();
    let package_fee = parent_fee + child_fee;
    let package_weight = parent_weight + child_weight;
    let package_feerate = package_fee / package_weight;

    println!(
        "Parent:  txid={}, fee={} sat, weight={} wu",
        parent_tx.compute_txid(),
        parent_fee.to_sat(),
        parent_weight.to_wu(),
    );
    println!(
        "Child:   txid={}, fee={} sat, weight={} wu",
        child_tx.compute_txid(),
        child_fee.to_sat(),
        child_weight.to_wu(),
    );
    println!(
        "Package: target={} sat/vB, actual={} sat/vB (fee={} sat, weight={} wu)",
        target_package_feerate.to_sat_per_vb_floor(),
        package_feerate.to_sat_per_vb_floor(),
        package_fee.to_sat(),
        package_weight.to_wu(),
    );
    println!("Child transaction hex: {}", serialize_hex(&child_tx));

    Ok(())
}

/// Builds the low-fee parent.
fn create_parent(
    wallet: &mut Wallet,
    funding_outpoint: OutPoint,
) -> anyhow::Result<(Transaction, OutPoint)> {
    let parent_script = wallet
        .reveal_next_address(KeychainKind::External)
        .script_pubkey();

    let mut builder = wallet.build_tx();
    builder
        .add_utxo(funding_outpoint)?
        .manually_selected_only()
        .drain_to(parent_script)
        .fee_absolute(PARENT_FEE)
        .set_exact_sequence(Sequence::MAX);

    let mut parent_psbt = builder.finish()?;
    wallet.sign(&mut parent_psbt, SignOptions::default())?;
    let parent_tx = parent_psbt.extract_tx()?;
    let parent_outpoint = OutPoint::new(parent_tx.compute_txid(), 0);

    wallet.apply_unconfirmed_txs([(parent_tx.clone(), 42)]);

    Ok((parent_tx, parent_outpoint))
}

/// Calculates the child fee for the parent+child package to meet the target fee rate.
fn required_child_fee_to_meet_target(
    parent_fee: Amount,
    parent_weight: Weight,
    child_weight: Weight,
    target_package_feerate: FeeRate,
) -> Amount {
    let package_weight = parent_weight + child_weight;
    let target_package_fee = target_package_feerate * package_weight;
    let required_child_fee = target_package_fee
        .checked_sub(parent_fee)
        .unwrap_or(Amount::ZERO);
    let min_child_fee = FeeRate::BROADCAST_MIN * child_weight;

    required_child_fee.max(min_child_fee)
}

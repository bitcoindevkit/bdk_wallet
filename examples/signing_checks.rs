//! Copyable checks for the former `SignOptions` defaults:
//!
//! - `trust_witness_utxo: false`: `check_prevouts`
//! - `allow_all_sighashes: false`: `check_sighashes`
//!
//! Use them if needed and call its helper before `Psbt::sign`.
//! Run with `cargo run --example signing_checks`.

use anyhow::Context;
use bdk_wallet::bitcoin::psbt::{self, PsbtSighashType};
use bdk_wallet::bitcoin::sighash::{EcdsaSighashType, TapSighashType};
use bdk_wallet::bitcoin::{
    Amount, FeeRate, NetworkKind, OutPoint, Psbt, Transaction, TxOut, secp256k1::Secp256k1,
};
use bdk_wallet::descriptor::IntoWalletDescriptor;
use bdk_wallet::miniscript::descriptor::KeyMapWrapper;
use bdk_wallet::test_utils::{get_funded_wallet_single, get_test_wpkh};
use bdk_wallet::{KeychainKind, Wallet};

fn main() -> anyhow::Result<()> {
    let secp = Secp256k1::new();
    let (descriptor, keymap) = get_test_wpkh().into_wallet_descriptor(&secp, NetworkKind::Test)?;
    let signer = KeyMapWrapper::from(keymap);

    // 1. Build a PSBT using a public descriptor and simulated funding.
    let (mut wallet, _) = get_funded_wallet_single(&descriptor.to_string());
    let recipient = wallet.peek_address(KeychainKind::External, 1);
    let mut builder = wallet.build_tx();
    builder
        .add_recipient(recipient.script_pubkey(), Amount::from_sat(10_000))
        .fee_rate(FeeRate::from_sat_per_vb(1).expect("valid fee rate"));
    let mut psbt = builder.finish()?;

    // 2. Check the PSBT before signing.
    // Equivalent to SignOptions::trust_witness_utxo: false.
    check_prevouts(&wallet, &psbt)?;
    // Equivalent to SignOptions::allow_all_sighashes: false.
    check_sighashes(&psbt)?;

    // 3. Sign with caller-owned keys.
    psbt.sign(&signer, wallet.secp_ctx())
        .map_err(|(_, errors)| anyhow::anyhow!("failed to sign PSBT: {errors:?}"))?;

    // 4. Finalize with the wallet.
    anyhow::ensure!(wallet.finalize_psbt(&mut psbt)?, "PSBT is not finalized");
    let tx = psbt.extract_tx()?;
    println!("Signed and finalized transaction: {}", tx.compute_txid());
    Ok(())
}

// Replacement for `SignOptions::trust_witness_utxo: false`.

/// Verifies prevouts using `non_witness_utxo` or wallet-held Taproot parents.
///
/// Why needed: A PSBT creator can falsify input amounts to hide excessive fees.
fn check_prevouts(wallet: &Wallet, psbt: &Psbt) -> anyhow::Result<()> {
    let inputs = psbt.inputs.iter().zip(&psbt.unsigned_tx.input);

    let all_amounts_committed = inputs.clone().all(|(input, txin)| {
        !is_anyone_can_pay(input) && is_verified_taproot(wallet, input, txin.previous_output)
    });

    for (i, (input, txin)) in inputs.enumerate() {
        // The signer checks supplied parents on unfinalized inputs even during the exemption.
        let check_supplied_parent = input.non_witness_utxo.is_some()
            && input.final_script_sig.is_none()
            && input.final_script_witness.is_none();
        if !all_amounts_committed || check_supplied_parent {
            check_input_amount(wallet, input, txin.previous_output, i)?;
        }
    }
    Ok(())
}

/// Whether the input's sighash type sets the `ANYONECANPAY` flag.
fn is_anyone_can_pay(input: &psbt::Input) -> bool {
    input.sighash_type.is_some_and(|ty| ty.to_u32() & 0x80 != 0)
}

/// Identifies Taproot using a wallet-held or txid-verified parent, never `witness_utxo` alone.
fn is_verified_taproot(wallet: &Wallet, input: &psbt::Input, outpoint: OutPoint) -> bool {
    let spends_p2tr = |parent: &Transaction| {
        parent
            .output
            .get(outpoint.vout as usize)
            .is_some_and(|prevout| prevout.script_pubkey.is_p2tr())
    };
    match wallet.tx_graph().get_tx(outpoint.txid) {
        Some(parent) => spends_p2tr(&parent),
        None => input
            .non_witness_utxo
            .as_ref()
            .is_some_and(|parent| parent.compute_txid() == outpoint.txid && spends_p2tr(parent)),
    }
}

/// Checks the supplied parent, or a wallet-held Taproot parent, against `witness_utxo`.
fn check_input_amount(
    wallet: &Wallet,
    input: &psbt::Input,
    outpoint: OutPoint,
    i: usize,
) -> anyhow::Result<()> {
    let vout = outpoint.vout as usize;
    let prevout: TxOut = match &input.non_witness_utxo {
        Some(parent) if parent.compute_txid() != outpoint.txid => {
            anyhow::bail!("input {i}: non_witness_utxo has the wrong txid");
        }
        Some(parent) => parent
            .output
            .get(vout)
            .cloned()
            .with_context(|| format!("input {i}: previous output index is out of range"))?,
        None => wallet
            .tx_graph()
            .get_tx(outpoint.txid)
            .and_then(|parent| parent.output.get(vout).cloned())
            .filter(|prevout| prevout.script_pubkey.is_p2tr())
            .with_context(|| format!("input {i}: missing non_witness_utxo"))?,
    };
    if let Some(witness_utxo) = &input.witness_utxo {
        anyhow::ensure!(
            witness_utxo.value == prevout.value,
            "input {i}: witness_utxo amount does not match the previous output"
        );
        anyhow::ensure!(
            witness_utxo.script_pubkey == prevout.script_pubkey,
            "input {i}: witness_utxo script_pubkey does not match the previous output"
        );
    }
    Ok(())
}

// === Former `SignOptions::allow_all_sighashes: false` ===

/// Allows unset sighashes, `ALL`, and `DEFAULT`.
///
/// Why needed: Other sighash types can allow inputs or outputs to change after signing.
fn check_sighashes(psbt: &Psbt) -> anyhow::Result<()> {
    let allowed: [PsbtSighashType; 2] = [
        EcdsaSighashType::All.into(), // same value as `TapSighashType::All`
        TapSighashType::Default.into(),
    ];
    for (i, input) in psbt.inputs.iter().enumerate() {
        if input.sighash_type.is_some_and(|ty| !allowed.contains(&ty)) {
            anyhow::bail!("input {i}: disallowed sighash type");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bdk_wallet::bitcoin::Script;
    use bdk_wallet::psbt::PsbtUtils;
    use bdk_wallet::test_utils::{
        ReceiveTo, get_funded_wallet, get_test_pkh, get_test_tr_single_sig,
        get_test_tr_single_sig_xprv, get_test_wpkh_and_change_desc, receive_output_in_latest_block,
        receive_output_to_address,
    };

    #[test]
    fn signs_and_finalizes_after_checks() {
        main().unwrap();
    }

    #[test]
    fn test_check_prevouts_rejects_mismatched_utxo() {
        let descriptor = get_test_wpkh();
        let (mut wallet, _) = get_funded_wallet_single(descriptor);
        receive_output_in_latest_block(&mut wallet, Amount::from_sat(50_000));
        let recipient = wallet.next_unused_address(KeychainKind::External);
        let mut builder = wallet.build_tx();
        builder.drain_to(recipient.script_pubkey()).drain_wallet();
        let psbt = builder.finish().unwrap();
        assert_eq!(psbt.inputs.len(), 2);

        for index in 0..psbt.inputs.len() {
            let mut mismatched = psbt.clone();
            mismatched.inputs[index]
                .witness_utxo
                .as_mut()
                .unwrap()
                .value -= Amount::from_sat(20_000);
            assert_eq!(mismatched.fee_amount(), None);
            assert_eq!(
                check_prevouts(&wallet, &mismatched)
                    .unwrap_err()
                    .to_string(),
                format!("input {index}: witness_utxo amount does not match the previous output")
            );
        }
    }

    #[test]
    fn test_check_prevouts_mixed_taproot_and_segwit_v0() {
        let (_, xpriv_change) = get_test_wpkh_and_change_desc();
        for (descriptor, change_descriptor) in [
            (get_test_tr_single_sig_xprv(), xpriv_change),
            (get_test_tr_single_sig(), get_test_wpkh()),
        ] {
            let (mut wallet, _) = get_funded_wallet(descriptor, change_descriptor);
            let change_address = wallet.next_unused_address(KeychainKind::Internal).address;
            receive_output_to_address(
                &mut wallet,
                change_address,
                Amount::from_sat(30_000),
                ReceiveTo::Mempool(0),
            );
            let recipient = wallet.next_unused_address(KeychainKind::External);
            let mut builder = wallet.build_tx();
            builder.drain_to(recipient.script_pubkey()).drain_wallet();
            let psbt = builder.finish().unwrap();
            assert_eq!(psbt.inputs.len(), 2);
            let input_index = |is_type: fn(&Script) -> bool| {
                psbt.inputs
                    .iter()
                    .position(|input| is_type(&input.witness_utxo.as_ref().unwrap().script_pubkey))
                    .unwrap()
            };
            let taproot_index = input_index(Script::is_p2tr);
            let segwit_v0_index = input_index(Script::is_p2wpkh);
            assert!(psbt.inputs[taproot_index].non_witness_utxo.is_none());
            assert!(psbt.inputs[segwit_v0_index].non_witness_utxo.is_some());

            let mut missing = psbt.clone();
            missing.inputs[segwit_v0_index].non_witness_utxo = None;
            assert_eq!(
                check_prevouts(&wallet, &missing).unwrap_err().to_string(),
                format!("input {segwit_v0_index}: missing non_witness_utxo")
            );

            let mut understated = psbt.clone();
            understated.inputs[taproot_index]
                .witness_utxo
                .as_mut()
                .unwrap()
                .value -= Amount::from_sat(20_000);
            assert_eq!(
                check_prevouts(&wallet, &understated)
                    .unwrap_err()
                    .to_string(),
                format!(
                    "input {taproot_index}: witness_utxo amount does not match the previous output"
                )
            );

            // A floating txout cannot authenticate the amount without its full transaction.
            let (mut unknown, _) = get_funded_wallet_single(get_test_pkh());
            unknown.insert_txout(
                psbt.unsigned_tx.input[taproot_index].previous_output,
                psbt.inputs[taproot_index].witness_utxo.clone().unwrap(),
            );
            assert_eq!(
                check_prevouts(&unknown, &psbt).unwrap_err().to_string(),
                format!("input {taproot_index}: missing non_witness_utxo")
            );
            check_prevouts(&wallet, &psbt).unwrap();
        }
    }

    #[test]
    fn test_check_prevouts_taproot_anyone_can_pay_authenticates_amounts() {
        let descriptor = get_test_tr_single_sig_xprv();
        let (mut wallet, _) = get_funded_wallet_single(descriptor);
        receive_output_in_latest_block(&mut wallet, Amount::from_sat(30_000));
        let recipient = wallet.next_unused_address(KeychainKind::External);
        let mut builder = wallet.build_tx();
        builder
            .drain_to(recipient.script_pubkey())
            .drain_wallet()
            .sighash(TapSighashType::AllPlusAnyoneCanPay.into());
        let psbt = builder.finish().unwrap();
        assert_eq!(psbt.inputs.len(), 2);
        assert!(
            psbt.inputs
                .iter()
                .all(|input| input.non_witness_utxo.is_none())
        );

        for index in 0..psbt.inputs.len() {
            let mut understated = psbt.clone();
            understated.inputs[index]
                .witness_utxo
                .as_mut()
                .unwrap()
                .value -= Amount::from_sat(20_000);
            assert_eq!(
                check_prevouts(&wallet, &understated)
                    .unwrap_err()
                    .to_string(),
                format!("input {index}: witness_utxo amount does not match the previous output")
            );
        }
        // Like the original test's allow_all_sighashes opt-in, omit check_sighashes here.
        check_prevouts(&wallet, &psbt).unwrap();
    }

    #[test]
    fn test_check_sighashes_rejects_nonstandard_sighash() {
        let sighash = EcdsaSighashType::NonePlusAnyoneCanPay;
        let descriptor = "wpkh(tprv8ZgxMBicQKsPd3EupYiPRhaMooHKUHJxNsTfYuScep13go8QFfHdtkG9nRkFGb7busX4isf6X9dURGCoKgitaApQ6MupRhZMcELAxTBRJgS/*)";
        let (mut wallet, _) = get_funded_wallet_single(descriptor);
        let addr = wallet.next_unused_address(KeychainKind::External);
        let mut builder = wallet.build_tx();
        builder
            .drain_to(addr.script_pubkey())
            .sighash(sighash.into())
            .drain_wallet();
        let psbt = builder.finish().unwrap();
        assert_eq!(
            check_sighashes(&psbt).unwrap_err().to_string(),
            "input 0: disallowed sighash type"
        );
    }

    #[test]
    fn test_check_sighashes_accepts_taproot_explicit_sighash_all() {
        let descriptor = get_test_tr_single_sig();
        let (mut wallet, _) = get_funded_wallet_single(descriptor);
        let addr = wallet.next_unused_address(KeychainKind::External);
        let mut builder = wallet.build_tx();
        builder
            .drain_to(addr.script_pubkey())
            .sighash(TapSighashType::All.into())
            .drain_wallet();
        let psbt = builder.finish().unwrap();
        check_sighashes(&psbt).unwrap();
    }

    #[test]
    fn test_check_sighashes_rejects_taproot_non_default_sighash() {
        let sighash = TapSighashType::NonePlusAnyoneCanPay;
        let descriptor = get_test_tr_single_sig();
        let (mut wallet, _) = get_funded_wallet_single(descriptor);
        let addr = wallet.next_unused_address(KeychainKind::External);
        let mut builder = wallet.build_tx();
        builder
            .drain_to(addr.script_pubkey())
            .sighash(sighash.into())
            .drain_wallet();
        let psbt = builder.finish().unwrap();
        assert_eq!(
            check_sighashes(&psbt).unwrap_err().to_string(),
            "input 0: disallowed sighash type"
        );
    }
}

//! Fuzz target: build transactions from a funded wallet with arbitrary parameters.
//!
//! The wallet is pre-funded with a deterministic UTXO. The fuzzer then drives
//! `build_tx` with arbitrary fee rates, recipient amounts, drain options and
//! sign options. `CreateTxError` and similar errors are expected; panics are not.
#![no_main]

use arbitrary::Arbitrary;
use bdk_wallet_fuzz::{create_wallet, funding_update};
use libfuzzer_sys::fuzz_target;

use bdk_wallet::{
    bitcoin::{Amount, FeeRate, Sequence},
    KeychainKind, SignOptions,
};

/// Bounded fee rate: 1 – 100_000 sat/kwu (~400 sat/vb max).
const MAX_FEE_SAT_KWU: u64 = 100_000;
/// Funding UTXO value.
const FUND_SATS: u64 = 100_000;

#[derive(Arbitrary, Debug)]
struct FuzzedTxParams {
    /// Satoshi amount to send to the recipient (before clamping).
    send_sats: u64,
    /// Fee rate in sat/kwu. Zero means "use default".
    fee_sat_kwu: u64,
    /// Use drain-wallet mode instead of a specific recipient amount.
    drain_wallet: bool,
    /// Enable RBF signalling.
    enable_rbf: bool,
    /// Try to sign the resulting PSBT.
    sign: bool,
    /// Recipient keychain index (0–9) so we can derive distinct addresses.
    recipient_index: u8,
    /// Use absolute fee instead of fee rate.
    use_absolute_fee: bool,
    /// Absolute fee in satoshis (only used when `use_absolute_fee` is true).
    absolute_fee_sats: u64,
}

fuzz_target!(|params: FuzzedTxParams| {
    let mut wallet = create_wallet();

    let update = funding_update(&wallet, FUND_SATS, 1);
    if wallet.apply_update(update).is_err() {
        return;
    }

    // Derive recipient and drain addresses from fixed indices to avoid borrow conflicts.
    let recipient_idx = params.recipient_index as u32 % 10 + 1; // indices 1–10
    let recipient_script = wallet
        .peek_address(KeychainKind::External, recipient_idx)
        .script_pubkey();
    let drain_script = wallet
        .peek_address(KeychainKind::External, recipient_idx + 10)
        .script_pubkey();

    let send_sats = params.send_sats.clamp(1, FUND_SATS);

    let mut builder = wallet.build_tx();

    if params.drain_wallet {
        builder.drain_wallet().drain_to(drain_script);
    } else {
        builder.add_recipient(recipient_script, Amount::from_sat(send_sats));
    }

    if params.enable_rbf {
        // RBF is signalled by setting nSequence <= 0xFFFFFFFD.
        builder.set_exact_sequence(Sequence::ENABLE_RBF_NO_LOCKTIME);
    }

    if params.use_absolute_fee {
        let abs_fee = params.absolute_fee_sats.min(FUND_SATS);
        builder.fee_absolute(Amount::from_sat(abs_fee));
    } else {
        let rate = params.fee_sat_kwu.clamp(1, MAX_FEE_SAT_KWU);
        builder.fee_rate(FeeRate::from_sat_per_kwu(rate));
    }

    let mut psbt = match builder.finish() {
        Ok(psbt) => psbt,
        Err(_) => return, // CreateTxError variants are expected
    };

    if params.sign {
        // Errors from sign are also expected (e.g. missing key, missing UTXO).
        let _ = wallet.sign(&mut psbt, SignOptions::default());
    }
});

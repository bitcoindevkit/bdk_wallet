#![allow(unused)]

use bdk_wallet::descriptor::IntoWalletDescriptor;
use bdk_wallet::signer::SignersContainer;
use bdk_wallet::test_utils::{
    get_test_pkh, get_test_tr_single_sig_xprv_and_change_desc, get_test_wpkh_and_change_desc,
};
use bdk_wallet::{SignOptions, Wallet};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::{Psbt, Transaction};
use miniscript::{
    Descriptor, DescriptorPublicKey,
    descriptor::{KeyMap, KeyMapWrapper},
};

/// The satisfaction size of P2WPKH is 108 WU =
/// 1 (elements in witness) + 1 (size) + 72 (signature + sighash) + 1 (size) + 33 (pubkey).
pub const P2WPKH_FAKE_PK_SIZE: usize = 33;
pub const P2WPKH_FAKE_SIG_SIZE: usize = 72;

/// The satisfaction size of P2PKH is 107 =
/// 1 (OP_PUSH) + 72 (signature + sighash) + 1 (OP_PUSH) + 33 (pubkey).
pub const P2PKH_FAKE_SCRIPT_SIG_SIZE: usize = 107;

pub fn parse_descriptor(s: &str) -> (Descriptor<DescriptorPublicKey>, KeyMap) {
    <Descriptor<DescriptorPublicKey>>::parse_descriptor(&Secp256k1::new(), s)
        .expect("failed to parse descriptor")
}

pub fn signers_from_descriptor(
    wallet: &Wallet,
    descriptor: impl IntoWalletDescriptor,
) -> SignersContainer {
    let (descriptor, keymap) = descriptor
        .into_wallet_descriptor(wallet.secp_ctx(), wallet.network().into())
        .expect("failed to parse signing descriptor");
    SignersContainer::build(keymap, &descriptor, wallet.secp_ctx())
}

pub fn keymap_from_descriptor(wallet: &Wallet, descriptor: impl IntoWalletDescriptor) -> KeyMap {
    let (_, keymap) = descriptor
        .into_wallet_descriptor(wallet.secp_ctx(), wallet.network().into())
        .expect("failed to parse signing descriptor");
    keymap
}

/// Signs and finalizes the inputs of `psbt` spending outputs of the test descriptors
/// ([`get_test_wpkh_and_change_desc`], [`get_test_tr_single_sig_xprv_and_change_desc`] and
/// [`get_test_pkh`]), then extracts the transaction.
///
/// Inputs that can't be signed with those keys (e.g. foreign inputs) are left as is.
pub fn sign_and_extract(wallet: &Wallet, psbt: Psbt) -> Transaction {
    let (wpkh, wpkh_change) = get_test_wpkh_and_change_desc();
    let (tr, tr_change) = get_test_tr_single_sig_xprv_and_change_desc();
    sign_and_extract_with(
        wallet,
        psbt,
        &[wpkh, wpkh_change, tr, tr_change, get_test_pkh()],
    )
}

/// Signs and finalizes the inputs of `psbt` with the private keys of `descriptors`, then
/// extracts the transaction.
///
/// Inputs that can't be signed with those keys (e.g. foreign inputs) are left as is.
pub fn sign_and_extract_with(wallet: &Wallet, mut psbt: Psbt, descriptors: &[&str]) -> Transaction {
    let mut keymap = KeyMap::new();
    for descriptor in descriptors {
        keymap.extend(keymap_from_descriptor(wallet, *descriptor));
    }
    let signer = KeyMapWrapper::from(keymap);
    // Ignore errors: some inputs may not be ours.
    let _ = psbt.sign(&signer, wallet.secp_ctx());
    let _ = wallet.finalize_psbt(&mut psbt, SignOptions::default());
    psbt.extract_tx_unchecked_fee_rate()
}

/// Signs the inputs of a hand-built `tx` that spend unspent wallet outputs with the test keys
/// (see [`sign_and_extract`]).
pub fn sign_tx(wallet: &Wallet, tx: Transaction) -> Transaction {
    let mut psbt = Psbt::from_unsigned_tx(tx).expect("tx must be unsigned");
    for (psbt_input, txin) in psbt.inputs.iter_mut().zip(&psbt.unsigned_tx.input) {
        if let Some(utxo) = wallet.get_utxo(txin.previous_output) {
            *psbt_input = wallet
                .get_psbt_input(utxo, None, true)
                .expect("failed to create psbt input");
        }
    }
    sign_and_extract(wallet, psbt)
}

/// Signs `tx` with [`sign_tx`] and inserts it as unconfirmed with
/// [`insert_tx`](bdk_wallet::test_utils::insert_tx).
pub fn insert_signed_tx(wallet: &mut Wallet, tx: Transaction) {
    let tx = sign_tx(wallet, tx);
    bdk_wallet::test_utils::insert_tx(wallet, tx);
}

/// Validate and return the transaction fee from a PSBT.
/// Panics if extraction fails, fee calculation fails, or if calculated fee doesn't match PSBT's
/// fee.
#[macro_export]
macro_rules! check_fee {
    ($wallet:expr, $psbt: expr) => {{
        let tx = $psbt.clone().extract_tx().expect("failed to extract tx");
        let tx_fee = $wallet.calculate_fee(&tx).expect("failed to calculate fee");
        assert_eq!(Some(tx_fee), $psbt.fee_amount());
        tx_fee
    }};
}

#[macro_export]
macro_rules! assert_fee_rate {
    ($psbt:expr, $fees:expr, $fee_rate:expr $( ,@dust_change $( $dust_change:expr )* )* $( ,@add_signature $( $add_signature:expr )* )* ) => ({
        let psbt = $psbt.clone();
        #[allow(unused_mut)]
        let mut tx = $psbt.clone().extract_tx().expect("failed to extract tx");

        $(
            $( $add_signature )*
                for txin in &mut tx.input {
                    txin.witness.push([0x00; common::P2WPKH_FAKE_SIG_SIZE]); // sig (72)
                    txin.witness.push([0x00; common::P2WPKH_FAKE_PK_SIZE]); // pk (33)
                }
        )*

        #[allow(unused_mut)]
        #[allow(unused_assignments)]
        let mut dust_change = false;
        $(
            $( $dust_change )*
                dust_change = true;
        )*

        let fee_amount = psbt.fee().expect("failed to calculate fee");

        assert_eq!(fee_amount, $fees);

        let tx_fee_rate = (fee_amount / tx.weight())
            .to_sat_per_kwu();
        let fee_rate = $fee_rate.to_sat_per_kwu();
        let half_default = FeeRate::BROADCAST_MIN.checked_div(2)
            .unwrap()
            .to_sat_per_kwu();

        if !dust_change {
            assert!(tx_fee_rate >= fee_rate && tx_fee_rate - fee_rate < half_default,
                "Expected fee rate of {:?}, the tx has {:?}", fee_rate, tx_fee_rate);
        } else {
            assert!(tx_fee_rate >= fee_rate,
                "Expected fee rate of at least {:?}, the tx has {:?}", fee_rate, tx_fee_rate);
        }
    });
}

#[macro_export]
macro_rules! assert_fee_rate_legacy {
    ($psbt:expr, $fees:expr, $fee_rate:expr $( ,@dust_change $( $dust_change:expr )* )* $( ,@add_signature $( $add_signature:expr )* )* ) => ({
        let psbt = $psbt.clone();
        #[allow(unused_mut)]
        let mut tx = $psbt.clone().extract_tx().expect("failed to extract tx");

        $(
            $( $add_signature )*
                for txin in &mut tx.input {
                    txin.script_sig = ScriptBuf::from_bytes([0x00; common::P2PKH_FAKE_SCRIPT_SIG_SIZE].to_vec()); // fake signature
                }
        )*

        #[allow(unused_mut)]
        #[allow(unused_assignments)]
        let mut dust_change = false;
        $(
            $( $dust_change )*
                dust_change = true;
        )*

        let fee_amount = psbt.fee().expect("failed to calculate fee");
        assert_eq!(fee_amount, $fees);

        let tx_fee_rate = (fee_amount / tx.weight())
            .to_sat_per_kwu();
        let fee_rate = $fee_rate.to_sat_per_kwu();
        let half_default = FeeRate::BROADCAST_MIN.checked_div(2)
            .unwrap()
            .to_sat_per_kwu();

        if !dust_change {
            assert!(tx_fee_rate >= fee_rate && tx_fee_rate - fee_rate < half_default,
                "Expected fee rate of {:?}, the tx has {:?}", fee_rate, tx_fee_rate);
        } else {
            assert!(tx_fee_rate >= fee_rate,
                "Expected fee rate of at least {:?}, the tx has {:?}", fee_rate, tx_fee_rate);
        }
    });
}

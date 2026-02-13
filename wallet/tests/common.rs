#![allow(unused)]

use bitcoin::secp256k1::Secp256k1;
use miniscript::{descriptor::KeyMap, Descriptor, DescriptorPublicKey};

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

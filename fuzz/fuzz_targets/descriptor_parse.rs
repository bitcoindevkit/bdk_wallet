//! Fuzz target: parse arbitrary byte strings as Bitcoin output descriptors.
//!
//! Exercises BDK's descriptor parsing and validation pipeline — including
//! miniscript type-checking, key derivation path validation, and checksum
//! verification — for both mainnet and testnet network kinds.
//! Any panic is a bug; parse errors are expected and ignored.
#![no_main]

use bdk_wallet::{
    bitcoin::{secp256k1::Secp256k1, NetworkKind},
    descriptor::IntoWalletDescriptor,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // IntoWalletDescriptor requires Secp256k1<All> for signing-capable context.
    let secp = Secp256k1::new();

    // Try both network kinds so we exercise both mainnet and testnet key parsing.
    let _ = s.into_wallet_descriptor(&secp, NetworkKind::Main);
    let _ = s.into_wallet_descriptor(&secp, NetworkKind::Test);
});

// TODO: expand into a complete guide for creating `KeychainKind` based wallets.
use bdk_wallet::{keyring::KeyRing, KeychainKind, Wallet};
use bitcoin::Network;

mod example_utils;

use example_utils::{get_descriptor, DESCRIPTORS};

fn main() {
    let descs = DESCRIPTORS.map(get_descriptor);
    let mut keyring = KeyRing::new(Network::Regtest, KeychainKind::External, descs[4].clone());
    keyring.add_descriptor(KeychainKind::Internal, descs[5].clone(), false);

    let mut wallet = Wallet::new(keyring);
    println!(
        "Address on external keychain: {}",
        wallet.reveal_next_default_address()
    );
}

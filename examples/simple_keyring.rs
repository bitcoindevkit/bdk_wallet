use bdk_wallet::keyring::KeyRing;
use bdk_wallet::{KeychainKind, Wallet};
use bitcoin::Network;

static EXTERNAL_DESCRIPTOR: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/0/*)";
static INTERNAL_DESCRIPTOR: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/1/*)";

// Simple KeyRing, allowing us to build a standard 2-descriptor wallet with receive and change
// keychains.

fn main() {
    let mut keyring: KeyRing<KeychainKind> = KeyRing::new(
        Network::Regtest,
        KeychainKind::External,
        EXTERNAL_DESCRIPTOR,
    )
    .unwrap();
    keyring
        .add_descriptor(KeychainKind::Internal, INTERNAL_DESCRIPTOR, false)
        .unwrap();

    let mut wallet = Wallet::new(keyring);
    let address = wallet.reveal_next_address(KeychainKind::External).unwrap();
    println!("Address at index {}: {}", address.index, address.address)
}

use bdk_wallet::{KeyRing, Wallet};
use bdk_wallet::bitcoin::Network;
use bdk_wallet::KeychainKind;

const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const OTHER_DESC_21: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/21/*)";
const OTHER_DESC_33: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/33/*)";
const OTHER_DESC_44: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/44/*)";

fn main() -> Result<(), anyhow::Error> {
    let mut keyring = KeyRing::new(EXTERNAL_DESC, Network::Testnet4);
    // keyring.add_other_descriptor(OTHER_DESC_21);
    // let keychains = keyring.list_keychains();
    // println!("{:?}", keychains);

    let wallet: Wallet = Wallet::new(keyring).create_wallet_no_persist()?;
    let address_1 = wallet.peek_address(KeychainKind::Default, 0).unwrap();
    // let address_2 = wallet.peek_address((KeychainKind::Other(), 0).unwrap();

    println!("Address {:?} at index {:?} on keychain {:?}", address_1.address, address_1.index, address_1.keychain);

    Ok(())
}

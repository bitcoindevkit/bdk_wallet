use bdk_chain::DescriptorId;
use bdk_wallet::Wallet;
use bdk_wallet::bitcoin::Network;
use bdk_wallet::KeychainKind;
use bdk_wallet::keyring::KeyRing;

const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const OTHER_DESC_21: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/21/*)";
const OTHER_DESC_31: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/33/*)";
const OTHER_DESC_41: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/44/*)";

fn main() -> Result<(), anyhow::Error> {
    // Create a keyring with a single, default descriptor (aka the KeychainKind::External from the 1.2.0 API)
    let mut keyring = KeyRing::new(EXTERNAL_DESC, Network::Testnet);

    // Add 3 new custom descriptors
    keyring.add_other_descriptor(OTHER_DESC_21);
    keyring.add_other_descriptor(OTHER_DESC_31);
    keyring.add_other_descriptor(OTHER_DESC_41);

    let keychain_ids: Vec<DescriptorId> = keyring.list_keychain_ids();
    println!("{:?}", keychain_ids);

    // Create the wallet and peek addresses on each of the descriptors
    let mut wallet: Wallet = Wallet::create(keyring).create_wallet_no_persist()?;
    let address_1 = wallet.peek_address(KeychainKind::Default, 0).unwrap();
    let address_2 = wallet.peek_address(KeychainKind::Other(keychain_ids[1]), 0).unwrap();
    let address_3 = wallet.peek_address(KeychainKind::Other(keychain_ids[2]), 0).unwrap();
    let address_4 = wallet.peek_address(KeychainKind::Other(keychain_ids[3]), 0).unwrap();

    println!("Address 1 {:?} at index {:?} on keychain {:?}", address_1.address, address_1.index, address_1.keychain);
    println!("Address 2 {:?} at index {:?} on keychain {:?}", address_2.address, address_2.index, address_2.keychain);
    println!("Address 3 {:?} at index {:?} on keychain {:?}", address_3.address, address_3.index, address_3.keychain);
    println!("Address 4 {:?} at index {:?} on keychain {:?}", address_4.address, address_4.index, address_4.keychain);

    let balance = wallet.balance();
    println!("Balance {:?}", balance);

    let revealed_address_1 = wallet.reveal_next_address(KeychainKind::Default);
    let revealed_address_2 = wallet.reveal_next_address(KeychainKind::Default);
    println!("Revealed next address {:?}", revealed_address_1);
    println!("Revealed next address {:?}", revealed_address_2);

    // Will error out because there is no change keychain defined
    // wallet.reveal_next_address(KeychainKind::Change).unwrap();

    Ok(())
}

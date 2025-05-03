use bdk_esplora::{esplora_client, EsploraExt};
use bdk_wallet::chain::DescriptorId;
use bdk_wallet::{KeyRing, Wallet};
use bdk_wallet::bitcoin::Network;
use bdk_wallet::KeychainKind;

const ESPLORA_URL: &str = "http://signet.bitcoindevkit.net";
const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const OTHER_DESC_21: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/21/*)";
const OTHER_DESC_31: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/31/*)";
const OTHER_DESC_41: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/41/*)";

fn main() -> Result<(), anyhow::Error> {
    // Create a keyring with a single, default descriptor (aka the KeychainKind::External from the 1.2.0 API)
    let mut keyring = KeyRing::new(EXTERNAL_DESC, Network::Signet);

    // Add 3 new custom descriptors
    keyring.add_other_descriptor(OTHER_DESC_21);
    keyring.add_other_descriptor(OTHER_DESC_31);
    keyring.add_other_descriptor(OTHER_DESC_41);

    let keychain_ids: Vec<DescriptorId> = keyring.list_keychain_ids();
    println!("{:?}", keychain_ids);

    // Create the wallet and peek addresses on each of the descriptors
    let mut wallet: Wallet = Wallet::new(keyring, Network::Signet).create_wallet_no_persist()?;
    let address_1 = wallet.peek_address(KeychainKind::Default, 0).unwrap();
    let address_2 = wallet.peek_address(KeychainKind::Other(keychain_ids[1]), 0).unwrap();
    let address_3 = wallet.peek_address(KeychainKind::Other(keychain_ids[2]), 0).unwrap();
    let address_4 = wallet.peek_address(KeychainKind::Other(keychain_ids[3]), 0).unwrap();

    println!("Address 1 {:?} at index {:?} on keychain {:?}", address_1.address, address_1.index, address_1.keychain);
    println!("Address 2 {:?} at index {:?} on keychain {:?}", address_2.address, address_2.index, address_2.keychain);
    println!("Address 3 {:?} at index {:?} on keychain {:?}", address_3.address, address_3.index, address_3.keychain);
    println!("Address 4 {:?} at index {:?} on keychain {:?}", address_4.address, address_4.index, address_4.keychain);

    let balance = wallet.balance();
    println!("Balance before sync {:?}", balance);

    let client = esplora_client::Builder::new(ESPLORA_URL).build_blocking();
    let full_scan_request = wallet.start_full_scan().build();
    let update = client.full_scan(full_scan_request, STOP_GAP, PARALLEL_REQUESTS)?;
    wallet.apply_update(update)?;

    let new_balance = wallet.balance();
    println!("Wallet balance after syncing: {}", new_balance.total());

    Ok(())
}

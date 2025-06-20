#![allow(unused)]
#![allow(clippy::print_stdout)]

use bdk_chain::DescriptorExt;
use bdk_chain::DescriptorId;
use bitcoin::secp256k1::Secp256k1;
use bitcoin::Network;
use miniscript::{Descriptor, DescriptorPublicKey};

use bdk_wallet::chain as bdk_chain;
use bdk_wallet::multi_keychain::{KeyRing, Wallet};
use bdk_wallet::rusqlite;

// This example shows how to create a BDK wallet from a `KeyRing`.

fn main() -> anyhow::Result<()> {
    let path = ".bdk_example_keyring.sqlite";
    let mut conn = rusqlite::Connection::open(path)?;

    let network = Network::Signet;

    let desc = "wpkh([83737d5e/84'/1'/1']tpubDCzuCBKnZA5TNKhiJnASku7kq8Q4iqcVF82JV7mHo2NxWpXkLRbrJaGA5ToE7LCuWpcPErBbpDzbdWKN8aTdJzmRy1jQPmZvnqpwwDwCdy7/<0;1>/*)";
    let desc2 = "tr([83737d5e/86'/1'/1']tpubDDR5GgtoxS8fNuSTJU6huqQKGzWshPaemb3UwFDoAXCsyakcQoRcFDMiGUVRX43Lofd7ZB82RcUvu1xnZ5oGZhbr43dRkY8xm2KGhpcq93o/<0;1>/*)";

    let default_did: DescriptorId =
        "6f3ba87443e825675b2b1cb8da505831422a7d214c515070570885180a1b2733".parse()?;

    let mut wallet = match Wallet::from_sqlite(&mut conn)? {
        Some(w) => w,
        None => {
            let mut keyring = KeyRing::new(network);
            for multipath_desc in [desc, desc2] {
                for (did, desc) in label_descriptors(multipath_desc) {
                    keyring.add_descriptor(did, desc);
                }
            }
            let mut wallet = Wallet::new(keyring);
            wallet.persist_to_sqlite(&mut conn)?;
            wallet
        }
    };

    let (indexed, addr) = wallet.reveal_next_address(default_did).unwrap();
    println!("Address: {:?} {}", indexed, addr);

    let changeset = wallet.persist_to_sqlite(&mut conn)?;
    println!("Change persisted: {}", changeset.is_some());

    Ok(())
}

/// Helper method to label descriptors by descriptor ID!
fn label_descriptors(
    s: &str,
) -> impl Iterator<Item = (DescriptorId, Descriptor<DescriptorPublicKey>)> {
    let desc = Descriptor::parse_descriptor(&Secp256k1::new(), s)
        .expect("failed to parse descriptor")
        .0;
    desc.into_single_descriptors()
        .expect("inavlid descriptor")
        .into_iter()
        .map(|desc| (desc.descriptor_id(), desc))
}

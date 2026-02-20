// This example demonstrates different address generation patterns with the new multi-keychain API:
// - Revealing addresses sequentially
// - Understanding address indices per keychain
// - Working with next_unused_address
// - Listing unspent outputs per keychain

use bdk_wallet::keyring::KeyRing;
use bdk_wallet::Wallet;
use bitcoin::Network;
use std::collections::BTreeMap;

const DESC_A: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/0/*)";
const DESC_B: &str = "wpkh([5bc5d243/84'/1'/0']tpubDCA4DcMLVSDifbfUxyJaVVAx57ztsVjke6DRYF95jFFgJqvzA9oENovVd7n34NNURmZxFNRB1VLGyDEqxvaZNXie3ZroEGFbeTS2xLYuaN1/0/*)";
const DESC_C: &str = "pkh([5bc5d243/44'/1'/0']tpubDDNQtvd8Sg1mXtSGtxRWEcgg7PbPwUSAyAmBonDSL3HLuutthe54Yih4XDYcywVdcduwqaQonpbTAGjjSh5kcLeCj5MTjYooa9ve2Npx6ho/0/*)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum KeychainIdentifier {
    A,
    B,
    C,
}

fn main() {
    println!();
    println!("┌────────────────────────────────────────────────────┐");
    println!("│                                                    │");
    println!("│  Generating Addresses With Multi-Keychain Wallets  │");
    println!("│                                                    │");
    println!("└────────────────────────────────────────────────────┘");

    // Create a wallet with three keychains
    let descriptors: BTreeMap<KeychainIdentifier, &str> = [
        (KeychainIdentifier::A, DESC_A),
        (KeychainIdentifier::B, DESC_B),
        (KeychainIdentifier::C, DESC_C),
    ]
    .into();

    let keyring = KeyRing::new_with_descriptors(Network::Signet, descriptors).unwrap();
    let mut wallet = Wallet::create(keyring).create_wallet_no_persist().unwrap();

    println!("Created wallet with 3 keychains (A, B, C)\n");

    println!("# 1. Custom keychain address generation");
    println!(
        "   Just like the 2.X Wallet, each keychain maintains its own revealed addresses indices."
    );
    println!("   You can reveal an address from any keychain.\n");

    println!("   Keychain A:");
    for _ in 0..5 {
        let addr = wallet.reveal_next_address(KeychainIdentifier::A).unwrap();
        println!("     - Index {}: {}", addr.index, addr.address);
    }

    println!("\n   Keychain B:");
    for _ in 0..3 {
        let addr = wallet.reveal_next_address(KeychainIdentifier::B).unwrap();
        println!("     - Index {}: {}", addr.index, addr.address);
    }

    println!("\n   Keychain C:");
    for _ in 0..2 {
        let addr = wallet.reveal_next_address(KeychainIdentifier::C).unwrap();
        println!("     - Index {}: {}", addr.index, addr.address);
    }

    println!("\n3. Using the last revealed index");
    for keychain in wallet.keychains().keys() {
        let last_revealed_index = wallet.derivation_index(*keychain);
        println!(
            "   Last revealed index on keychain {:?}: {:?}",
            keychain, last_revealed_index
        );
    }
}

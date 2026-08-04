// Adapted from bitcoindevkit/bdk_wallet#318 (examples/multi_keychain/address_generation.rs).
//
// Address generation across several keychains:
// - revealing addresses from any keychain
// - each keychain keeping its own derivation index
// - `next_unused_address` vs `reveal_next_address`
// - reading back the last revealed index per keychain

use bdk_wallet::bitcoin::Network;
use bdk_wallet::{KeyRing, Wallet};

const DESC_A: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/0/*)";
const DESC_B: &str = "wpkh([5bc5d243/84'/1'/0']tpubDCA4DcMLVSDifbfUxyJaVVAx57ztsVjke6DRYF95jFFgJqvzA9oENovVd7n34NNURmZxFNRB1VLGyDEqxvaZNXie3ZroEGFbeTS2xLYuaN1/0/*)";
const DESC_C: &str = "pkh([5bc5d243/44'/1'/0']tpubDDNQtvd8Sg1mXtSGtxRWEcgg7PbPwUSAyAmBonDSL3HLuutthe54Yih4XDYcywVdcduwqaQonpbTAGjjSh5kcLeCj5MTjYooa9ve2Npx6ho/0/*)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Keychain {
    A,
    B,
    C,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut keyring = KeyRing::new(Network::Signet, Keychain::A, DESC_A)?;
    keyring.add_descriptor(Keychain::B, DESC_B)?;
    keyring.add_descriptor(Keychain::C, DESC_C)?;

    let mut wallet = Wallet::create(keyring).create_wallet_no_persist();

    println!("Created a wallet with 3 keychains (A, B, C)\n");

    // Each keychain keeps its own derivation index, exactly as the external and internal
    // keychains do on a standard two-keychain wallet.
    println!("1. Revealing addresses from each keychain");
    for (keychain, count) in [(Keychain::A, 5), (Keychain::B, 3), (Keychain::C, 2)] {
        println!("   Keychain {keychain:?}:");
        for _ in 0..count {
            let addr = wallet.reveal_next_address(keychain);
            println!("     - index {}: {}", addr.index, addr.address);
        }
    }

    // `next_unused_address` hands back the lowest revealed address that has not been used yet,
    // rather than revealing a new one. With no transactions in this wallet, nothing has been used,
    // so it returns index 0 instead of advancing past what we revealed above.
    println!("\n2. next_unused_address does not advance the index");
    for keychain in [Keychain::A, Keychain::B, Keychain::C] {
        let addr = wallet.next_unused_address(keychain);
        println!("   Keychain {keychain:?}: index {}", addr.index);
    }

    println!("\n3. Last revealed index per keychain");
    for (keychain, _descriptor) in wallet.keychains() {
        // `derivation_index` is `None` until something has actually been revealed.
        println!(
            "   Keychain {keychain:?}: {:?}",
            wallet.derivation_index(keychain)
        );
    }

    Ok(())
}

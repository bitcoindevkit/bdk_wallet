// This example demonstrates how to think about migrating from the old KeychainKind-based
// API to the new generic keychain API. It shows that KeychainKind still works perfectly
// as a keychain identifier - it's just one option among many.

use bdk_wallet::keyring::KeyRing;
use bdk_wallet::{KeychainKind, Wallet};
use bitcoin::Network;

const EXTERNAL_DESC: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/0/*)";
const INTERNAL_DESC: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/1/*)";

#[rustfmt::skip]
fn main() {
    println!();
    println!("┌────────────────────────────────────────────┐");
    println!("│                                            │");
    println!("│  Migration from 2.X API with KeychainKind  │");
    println!("│                                            │");
    println!("└────────────────────────────────────────────┘");

    println!("\nThe old API had a fixed 2-keychain model using KeychainKind::External and");
    println!("KeychainKind::Internal. The new API is generic over the keychain identifier,");
    println!("but KeychainKind still works perfectly for the common 2-descriptor case!\n");

    // ============================================================================
    // OLD PATTERN (conceptually - the actual old API is different)
    // ============================================================================
    println!("OLD PATTERN (2-descriptor wallet with External/Internal):");
    println!("  let wallet = Wallet::new(external_desc, internal_desc, ...)?;");
    println!("  let addr = wallet.get_address(AddressIndex::New)?;");
    println!("  let change = wallet.get_internal_address(AddressIndex::New)?;\n");

    // ============================================================================
    // NEW PATTERN (same functionality, more explicit)
    // ============================================================================
    println!("NEW PATTERN (equivalent 2-descriptor wallet):");
    println!("  let mut keyring = KeyRing::new(network, KeychainKind::External, external_desc)?;");
    println!("  keyring.add_descriptor(KeychainKind::Internal, internal_desc, false)?;");
    println!("  let wallet = Wallet::create(keyring).create_wallet_no_persist();");
    println!("  let addr = wallet.reveal_next_address(KeychainKind::External)?;");
    println!("  let change = wallet.reveal_next_address(KeychainKind::Internal)?;\n");

    println!("NEW APIs");
    // Create a traditional 2-keychain wallet using KeychainKind
    let mut keyring: KeyRing<KeychainKind> = KeyRing::new(
        Network::Signet,
        KeychainKind::External,
        EXTERNAL_DESC,
    )
    .unwrap();

    keyring
        .add_descriptor(KeychainKind::Internal, INTERNAL_DESC, false)
        .unwrap();

    let mut wallet = Wallet::create(keyring).create_wallet_no_persist();

    // Generate addresses just like before
    println!("Generating addresses:");
    let receive = wallet.reveal_next_address(KeychainKind::External).unwrap();
    println!("  Receive (External): {} at index {}", receive.address, receive.index);

    let change = wallet.reveal_next_address(KeychainKind::Internal).unwrap();
    println!("  Change  (Internal): {} at index {}\n", change.address, change.index);

    // The default keychain concept is new
    println!("NEW FEATURE: Default keychain");
    println!("  Default: {:?}", wallet.default_keychain());
    let default_addr = wallet.reveal_next_default_address();
    println!("  Default address: {} (keychain: {:?})\n", default_addr.address, default_addr.keychain);

    // Show that all the wallet operations work the same way
    println!("Standard operations work as expected:");
    println!("  Network: {:?}", wallet.network());
    println!("  Keychains: {}", wallet.keychains().len());
    println!("  Balance: {:?}\n", wallet.balance());

    // ============================================================================
    // KEY DIFFERENCES AND BENEFITS
    // ============================================================================
    println!("KEY DIFFERENCES:");
    println!("  1. Explicit keychain creation via KeyRing (more flexible)");
    println!("  2. reveal_next_address() takes an explicit keychain parameter");
    println!("  3. reveal_next_default_address() for convenience");
    println!("  4. Can now use ANY number of keychains!\n");

    println!("BENEFITS OF NEW API:");
    println!("  ✓ Not limited to 2 descriptors (External/Internal)");
    println!("  ✓ Can use any type as keychain identifier (KeychainKind, String, custom enum, etc.)");
    println!("  ✓ More explicit about which keychain you're using");
    println!("  ✓ Better separation of concerns (KeyRing vs Wallet)\n");

    println!("MIGRATION STRATEGY:");
    println!("  1. If you only need External/Internal, keep using KeychainKind as your keychain identifier");
    println!("  2. Change wallet construction to use KeyRing::new() + add_descriptor()");
    println!("  3. Update address generation calls to use reveal_next_address(keychain)");
    println!("  4. If you want to expand beyond 2 keychains, define a custom enum");
}

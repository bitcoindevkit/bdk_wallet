// This example demonstrates how to track balances across different keychains.
// This is useful if users want to query the balances of different keychains within a single wallet.

use bdk_wallet::keyring::KeyRing;
use bdk_wallet::Wallet;
use bitcoin::Network;
use std::collections::BTreeMap;

// Different descriptors for different purposes
const SAVINGS_DESC: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/0/*)";
const SPENDING_DESC: &str = "wpkh([5bc5d243/84'/1'/0']tpubDCA4DcMLVSDifbfUxyJaVVAx57ztsVjke6DRYF95jFFgJqvzA9oENovVd7n34NNURmZxFNRB1VLGyDEqxvaZNXie3ZroEGFbeTS2xLYuaN1/0/*)";
const DONATIONS_DESC: &str = "pkh([5bc5d243/44'/1'/0']tpubDDNQtvd8Sg1mXtSGtxRWEcgg7PbPwUSAyAmBonDSL3HLuutthe54Yih4XDYcywVdcduwqaQonpbTAGjjSh5kcLeCj5MTjYooa9ve2Npx6ho/1/*)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AccountType {
    Savings,
    Spending,
    Donations,
}

#[rustfmt::skip]
fn main() {
    println!();
    println!("┌──────────────────────────────────────────┐");
    println!("│                                          │");
    println!("│  Keeping track of balances per keychain  │");
    println!("│                                          │");
    println!("└──────────────────────────────────────────┘");

    // Create a wallet with multiple keychains representing different accounts
    let descriptors: BTreeMap<AccountType, &str> = [
        (AccountType::Savings, SAVINGS_DESC),
        (AccountType::Spending, SPENDING_DESC),
        (AccountType::Donations, DONATIONS_DESC),
    ]
    .into();

    let keyring = KeyRing::new_with_descriptors(Network::Signet, descriptors, None).unwrap();
    let mut wallet = Wallet::create(keyring).create_wallet_no_persist();

    println!("Created wallet with 3 keychains:\n");

    // List all keychains
    println!("1. All configured keychains:");
    for (keychain, descriptor) in wallet.keychains() {
        println!("   {:?}: {}", keychain, descriptor);
    }

    // Generate addresses for each keychain
    println!("\n2. Generating receive addresses:");
    let savings_addr = wallet.reveal_next_address(AccountType::Savings).unwrap();
    println!("   Savings   (index {}): {}", savings_addr.index, savings_addr.address);

    let spending_addr = wallet.reveal_next_address(AccountType::Spending).unwrap();
    println!("   Spending  (index {}): {}", spending_addr.index, spending_addr.address);

    let donations_addr = wallet.reveal_next_address(AccountType::Donations).unwrap();
    println!("   Donations (index {}): {}\n", donations_addr.index, donations_addr.address);

    // Check overall balance
    println!("3. Current balances:");
    let total_balance = wallet.balance();
    println!("   Total wallet balance:");
    println!("   - Confirmed:   {} sats", total_balance.confirmed);
    println!("   - Unconfirmed: {} sats", total_balance.immature);
    println!("   - Trusted:     {} sats\n", total_balance.trusted_pending);

    // Check balance for specific keychain ranges
    println!("4. Balance by account type:");

    // Balance for just savings
    let savings_balance = wallet.keychain_balance(AccountType::Savings..=AccountType::Savings);
    println!("   Savings account:");
    println!("   - Total: {} sats", savings_balance.total());

    // Balance for spending
    let spending_balance = wallet.keychain_balance(AccountType::Spending..=AccountType::Spending);
    println!("   Spending account:");
    println!("   - Total: {} sats", spending_balance.total());

    // Balance for donations
    let donations_balance = wallet.keychain_balance(AccountType::Donations..=AccountType::Donations);
    println!("   Donations account:");
    println!("   - Total: {} sats\n", donations_balance.total());

    // Show how to query multiple keychains at once
    println!("5. Combined balances of 2 keychains:");
    let non_savings = wallet.keychain_balance(AccountType::Spending..=AccountType::Donations);
    println!("   Spending + Donations:");
    println!("   - Total: {} sats\n", non_savings.total());
}

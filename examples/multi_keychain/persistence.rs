// This example demonstrates wallet persistence and recovery patterns with the new KeyRing API.
// It shows how to:
// 1. Create a wallet with multiple keychains
// 2. Persist it to a database
// 3. Load it back from the database

use std::collections::BTreeMap;

use bdk_chain::rusqlite;
use bdk_chain::rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use bdk_wallet::keyring::KeyRing;
use bdk_wallet::{LoadParams, Wallet};
use bitcoin::Network;

// Different descriptors for different purposes
const SAVINGS_DESC: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/0/*)";
const SPENDING_DESC: &str = "wpkh([5bc5d243/84'/1'/0']tpubDCA4DcMLVSDifbfUxyJaVVAx57ztsVjke6DRYF95jFFgJqvzA9oENovVd7n34NNURmZxFNRB1VLGyDEqxvaZNXie3ZroEGFbeTS2xLYuaN1/0/*)";
const DONATIONS_DESC: &str = "pkh([5bc5d243/44'/1'/0']tpubDDNQtvd8Sg1mXtSGtxRWEcgg7PbPwUSAyAmBonDSL3HLuutthe54Yih4XDYcywVdcduwqaQonpbTAGjjSh5kcLeCj5MTjYooa9ve2Npx6ho/1/*)";

const DONATIONS_WRONG_DESC: &str = "pkh([5bc5d243/44'/1'/0']tpubDDNQtvd8Sg1mXtSGtxRWEcgg7PbPwUSAyAmBonDSL3HLuutthe54Yih4XDYcywVdcduwqaQonpbTAGjjSh5kcLeCj5MTjYooa9ve2Npx6ho/2/*)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AccountType {
    Savings,
    Spending,
    Donations,
}

impl FromSql for AccountType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(match value.as_str()? {
            "savings" => AccountType::Savings,
            "spending" => AccountType::Spending,
            "donations" => AccountType::Donations,
            other => panic!("Unknown AccountType: {}", other),
        })
    }
}

impl ToSql for AccountType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            AccountType::Savings => "savings".into(),
            AccountType::Spending => "spending".into(),
            AccountType::Donations => "donations".into(),
        })
    }
}

#[rustfmt::skip]
fn main() {
    println!();
    println!("┌───────────────────────────────────┐");
    println!("│                                   │");
    println!("│  Wallet persistence and recovery  │");
    println!("│                                   │");
    println!("└───────────────────────────────────┘");

    let db_path = ".multi_keychain_persistence_example.sqlite3";

    // Part 1: Create and persist a new wallet
    println!("\n1. Creating a new wallet with Savings, Spending, and Donations keychains...");
    {
        let mut conn = rusqlite::Connection::open(db_path).unwrap();

        // Create a keyring with three descriptors for different account types
        let descriptors: BTreeMap<AccountType, &str> = [
            (AccountType::Savings, SAVINGS_DESC),
            (AccountType::Spending, SPENDING_DESC),
            (AccountType::Donations, DONATIONS_DESC),
        ]
        .into();

        let keyring = KeyRing::new_with_descriptors(Network::Regtest, descriptors, None).unwrap();

        // Create the wallet
        let mut wallet = Wallet::create(keyring).create_wallet(&mut conn).unwrap();

        // Generate some addresses
        let savings_addr = wallet.reveal_next_address(AccountType::Savings).unwrap();
        let spending_addr = wallet.reveal_next_address(AccountType::Spending).unwrap();
        let donations_addr = wallet.reveal_next_address(AccountType::Donations).unwrap();

        println!("   Generated addresses:");
        println!("   - Savings address 0:   {}", savings_addr.address);
        println!("   - Spending address 0:  {}", spending_addr.address);
        println!("   - Donations address 0: {}", donations_addr.address);

        // Persist the wallet
        wallet.persist(&mut conn).unwrap();
        println!("   Wallet persisted to database\n");
    }

    // Part 2: Demonstrate recovery scenario
    println!("2. Simple load from database...");
    {
        let mut conn = rusqlite::Connection::open(db_path).unwrap();

        let params = LoadParams::<AccountType>::new()
            .check_network(Network::Regtest);

        match params.load_wallet(&mut conn).unwrap() {
            Some(wallet) => {
                println!("   Wallet recovered successfully!");
                println!("   The wallet has {} keychains", wallet.keychains().len());

                // Show all keychains
                for (keychain, descriptor) in wallet.keychains() {
                    println!("   - {:?}: {}", keychain, descriptor);
                }
            }
            None => {
                println!("   No wallet found");
            }
        }
    }

    // Part 3: Load the wallet from persistence and perform checks
    println!("\n3. Loading wallet from database and cross-check all data...");
    println!("   This will error out if the wallet does not match expectations.");
    println!("   In this case the Donations keychain descriptor is intentionally wrong.\n");
    {
        let mut conn = rusqlite::Connection::open(db_path).unwrap();

        // Set up load parameters - these verify the wallet matches expectations
        let params = LoadParams::<AccountType>::new()
            .check_default(AccountType::Savings)
            .check_descriptor(AccountType::Savings, Some(SAVINGS_DESC))
            .check_descriptor(AccountType::Spending, Some(SPENDING_DESC))
            .check_descriptor(AccountType::Donations, Some(DONATIONS_WRONG_DESC))
            .check_genesis_hash(bitcoin::constants::genesis_block(Network::Regtest).block_hash())
            .check_network(Network::Regtest);

        match params.load_wallet(&mut conn) {
            Ok(Some(wallet)) => {
                println!("   Wallet successfully loaded!");
                println!("   Network: {:?}", wallet.network());
                println!("   Default keychain: {:?}", wallet.default_keychain());
            }
            Ok(None) => {
                println!("   No wallet found in database.");
            }
            Err(e) => {
                println!("   Error loading wallet: {:?}", e);
            }
        }
    }
}

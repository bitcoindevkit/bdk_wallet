// Adapted from bitcoindevkit/bdk_wallet#318 (examples/multi_keychain/persistence.rs).
//
// Persisting and reloading a wallet that tracks several keychains:
// 1. create a wallet with three keychains and persist it to sqlite
// 2. load it back
// 3. load it again with expectations, and watch the check fail when one does not match
//
// A custom keychain type has to be storable, so it implements `ToSql` and `FromSql`. Those two
// impls are the only extra work a custom `K` needs in order to persist.

use bdk_wallet::bitcoin::Network;
use bdk_wallet::chain::rusqlite;
use bdk_wallet::chain::rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use bdk_wallet::{KeyRing, LoadParams, Wallet};

const SAVINGS_DESC: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/0/*)";
const SPENDING_DESC: &str = "wpkh([5bc5d243/84'/1'/0']tpubDCA4DcMLVSDifbfUxyJaVVAx57ztsVjke6DRYF95jFFgJqvzA9oENovVd7n34NNURmZxFNRB1VLGyDEqxvaZNXie3ZroEGFbeTS2xLYuaN1/0/*)";
const DONATIONS_DESC: &str = "pkh([5bc5d243/44'/1'/0']tpubDDNQtvd8Sg1mXtSGtxRWEcgg7PbPwUSAyAmBonDSL3HLuutthe54Yih4XDYcywVdcduwqaQonpbTAGjjSh5kcLeCj5MTjYooa9ve2Npx6ho/1/*)";

// Deliberately not the descriptor we stored, to show the load-time check rejecting a mismatch.
const DONATIONS_WRONG_DESC: &str = "pkh([5bc5d243/44'/1'/0']tpubDDNQtvd8Sg1mXtSGtxRWEcgg7PbPwUSAyAmBonDSL3HLuutthe54Yih4XDYcywVdcduwqaQonpbTAGjjSh5kcLeCj5MTjYooa9ve2Npx6ho/2/*)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Account {
    Savings,
    Spending,
    Donations,
}

// How the keychain is written to, and read back from, the database. The stored string is what
// identifies the keychain across restarts, so it must stay stable for the life of the wallet.
impl ToSql for Account {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            Account::Savings => "savings".into(),
            Account::Spending => "spending".into(),
            Account::Donations => "donations".into(),
        })
    }
}

impl FromSql for Account {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "savings" => Ok(Account::Savings),
            "spending" => Ok(Account::Spending),
            "donations" => Ok(Account::Donations),
            other => Err(rusqlite::types::FromSqlError::Other(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown account keychain: {other}"),
                ),
            ))),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A temporary database, so the example is repeatable.
    let db_dir = tempfile::tempdir()?;
    let db_path = db_dir.path().join("multi_keychain_persistence.sqlite3");

    // 1. Create and persist
    println!("1. Creating a wallet with Savings, Spending and Donations keychains");
    {
        let mut conn = rusqlite::Connection::open(&db_path)?;

        let mut keyring = KeyRing::new(Network::Regtest, Account::Savings, SAVINGS_DESC)?;
        keyring.add_descriptor(Account::Spending, SPENDING_DESC)?;
        keyring.add_descriptor(Account::Donations, DONATIONS_DESC)?;

        let mut wallet = Wallet::create(keyring).create_wallet(&mut conn)?;

        for account in [Account::Savings, Account::Spending, Account::Donations] {
            let addr = wallet.reveal_next_address(account);
            println!("   {:?} address {}: {}", account, addr.index, addr.address);
        }

        wallet.persist(&mut conn)?;
        println!("   Persisted.\n");
    }

    // 2. Load it back
    println!("2. Loading the wallet back from the database");
    {
        let mut conn = rusqlite::Connection::open(&db_path)?;

        let params = LoadParams::<Account>::new().check_network(Network::Regtest);
        match params.load_wallet(&mut conn)? {
            Some(wallet) => {
                println!("   Recovered {} keychains:", wallet.keychains().count());
                for (account, descriptor) in wallet.keychains() {
                    // Revealed indices survive the round-trip along with the descriptors.
                    println!(
                        "   - {:?} (last revealed index {:?}): {}",
                        account,
                        wallet.derivation_index(account),
                        descriptor
                    );
                }
            }
            None => println!("   No wallet found."),
        }
        println!();
    }

    // 3. Load with expectations, one of which is wrong on purpose
    println!("3. Loading with expectations, where the Donations descriptor is intentionally wrong");
    {
        let mut conn = rusqlite::Connection::open(&db_path)?;

        let params = LoadParams::<Account>::new()
            .check_network(Network::Regtest)
            .check_genesis_hash(
                bdk_wallet::bitcoin::constants::genesis_block(Network::Regtest).block_hash(),
            )
            // Each of these asserts "this keychain must be loaded, and must be this descriptor".
            .descriptor(Account::Savings, Some(SAVINGS_DESC))
            .descriptor(Account::Spending, Some(SPENDING_DESC))
            .descriptor(Account::Donations, Some(DONATIONS_WRONG_DESC));

        match params.load_wallet(&mut conn) {
            Ok(Some(_wallet)) => println!("   Loaded (unexpected — the check should have failed)"),
            Ok(None) => println!("   No wallet found."),
            Err(e) => println!("   Rejected, as expected:\n     {e}"),
        }
    }

    Ok(())
}

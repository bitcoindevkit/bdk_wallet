//! Example demonstrating how to configure EsploraClient with custom timeout settings.

use bdk_esplora::{esplora_client, EsploraExt};
use bdk_wallet::{bitcoin::Network, rusqlite::Connection, KeychainKind, Wallet};
use std::{collections::BTreeSet, io::Write};

const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

const DB_PATH: &str = "bdk-example-esplora-timeout.sqlite";
const NETWORK: Network = Network::Testnet4;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ESPLORA_URL: &str = "https://mempool.space/testnet4/api";

const TIMEOUT_SECS: u64 = 30;
const MAX_RETRIES: usize = 3;

fn main() -> Result<(), anyhow::Error> {
    let mut db = Connection::open(DB_PATH)?;
    let wallet_opt = Wallet::load()
        .descriptor(KeychainKind::External, Some(EXTERNAL_DESC))
        .descriptor(KeychainKind::Internal, Some(INTERNAL_DESC))
        .extract_keys()
        .check_network(NETWORK)
        .load_wallet(&mut db)?;
    let mut wallet = match wallet_opt {
        Some(wallet) => wallet,
        None => Wallet::create(EXTERNAL_DESC, INTERNAL_DESC)
            .network(NETWORK)
            .create_wallet(&mut db)?,
    };

    let address = wallet.next_unused_address(KeychainKind::External);
    wallet.persist(&mut db)?;
    println!(
        "Next unused address: ({}) {}",
        address.index, address.address
    );

    let balance = wallet.balance();
    println!("Wallet balance before syncing: {}", balance.total());

    // Configure EsploraClient with custom timeout and retry settings.
    // Available builder options:
    // - timeout(secs): Socket timeout in seconds
    // - max_retries(count): Retries on HTTP codes 408, 425, 429, 500, 502, 503, 504
    // - proxy(url): Proxy server URL
    // - header(key, value): Custom HTTP header
    println!(
        "Creating Esplora client with {TIMEOUT_SECS}s timeout and {MAX_RETRIES} max retries..."
    );
    let client = esplora_client::Builder::new(ESPLORA_URL)
        .timeout(TIMEOUT_SECS)
        .max_retries(MAX_RETRIES)
        .build_blocking();

    println!("Starting full sync...");
    let request = wallet.start_full_scan().inspect({
        let mut stdout = std::io::stdout();
        let mut once = BTreeSet::<KeychainKind>::new();
        move |keychain, spk_i, _| {
            if once.insert(keychain) {
                print!("\nScanning keychain [{keychain:?}] ");
            }
            print!(" {spk_i:<3}");
            stdout.flush().expect("must flush")
        }
    });

    match client.full_scan(request, STOP_GAP, PARALLEL_REQUESTS) {
        Ok(update) => {
            wallet.apply_update(update)?;
            wallet.persist(&mut db)?;

            let balance = wallet.balance();
            println!("\nWallet balance after syncing: {}", balance.total());
        }
        Err(e) => {
            eprintln!("\nSync failed: {e}");
            eprintln!(
                "Tips: increase timeout/max_retries, check connectivity, or try another server"
            );
            return Err(e.into());
        }
    }

    println!("\nFetching current block height...");
    match client.get_height() {
        Ok(height) => println!("Current block height: {height}"),
        Err(e) => eprintln!("Failed to get block height: {e}"),
    }

    println!("\nFetching fee estimates...");
    match client.get_fee_estimates() {
        Ok(estimates) => {
            println!("Fee estimates (sat/vB):");
            for (target, rate) in estimates.iter().take(5) {
                println!("  {} blocks: {:.1} sat/vB", target, rate);
            }
        }
        Err(e) => {
            eprintln!("Failed to get fee estimates: {e}");
        }
    }

    Ok(())
}

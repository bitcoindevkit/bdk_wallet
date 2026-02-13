#![no_main]

use libfuzzer_sys::fuzz_target;

use bdk_wallet::{rusqlite::Connection, Wallet};
use bdk_wallet_fuzz::arbitrary_types_optimized::{
    OptimizedFuzzInput, EXTERNAL_DESCRIPTOR, INTERNAL_DESCRIPTOR, NETWORK
};

fuzz_target!(|input: OptimizedFuzzInput| {
    // Create an in-memory database connection
    let mut db_conn = Connection::open_in_memory()
        .expect("Should start an in-memory database connection successfully!");

    // Create the initial wallet
    let wallet = Wallet::create(EXTERNAL_DESCRIPTOR, INTERNAL_DESCRIPTOR)
        .network(NETWORK)
        .create_wallet(&mut db_conn);

    // If wallet creation fails, skip this input
    let mut wallet = match wallet {
        Ok(wallet) => wallet,
        Err(_) => return,
    };

    // Execute all operations from the fuzz input
    // Errors are expected and handled gracefully
    let _ = input.execute(&mut wallet);
});
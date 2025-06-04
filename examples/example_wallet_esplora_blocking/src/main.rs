#![allow(unused)]

use std::{collections::BTreeSet, io::Write};

use bdk_wallet::{
    bitcoin::{Amount, Network},
    rusqlite::Connection,
    KeyRing, KeychainKind, PersistedWallet, SignOptions, Wallet, WalletParams,
};

const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

const DB_PATH: &str = ".bdk_example_wallet_esplora_blocking.sqlite";
const NETWORK: Network = Network::Signet;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ESPLORA_URL: &str = "http://signet.bitcoindevkit.net";

fn main() -> Result<(), anyhow::Error> {
    let mut conn = Connection::open(DB_PATH)?;

    // Load from database, or if empty create new.
    let mut wallet = match PersistedWallet::load_from_changeset(&mut conn)? {
        Some(w) => w,
        None => PersistedWallet::with_params(
            &mut conn,
            WalletParams {
                keyring: KeyRing::new(EXTERNAL_DESC, NETWORK),
                ..Default::default()
            },
        )?,
    };

    let addr = wallet.reveal_next_default_address();
    wallet.persist(&mut conn)?;

    dbg!(&addr);

    Ok(())
}

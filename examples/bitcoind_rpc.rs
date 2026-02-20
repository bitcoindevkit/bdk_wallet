#![allow(unused)]
use bdk_bitcoind_rpc::{
    bitcoincore_rpc::{Auth, Client, RpcApi},
    Emitter, MempoolEvent,
};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{
    bitcoin::{Block, Network},
    keyring::KeyRing,
    KeychainKind, LoadParams, PersistedWallet, Wallet,
};
use clap::{self, Parser};
use std::{
    path::PathBuf,
    sync::{mpsc::sync_channel, Arc},
    thread::spawn,
    time::Instant,
};

/// Bitcoind RPC example using `bdk_wallet::Wallet`.
///
/// This syncs the chain block-by-block and prints the current balance, transaction count and UTXO
/// count.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
pub struct Args {
    /// Wallet descriptor
    #[clap(env = "DESCRIPTOR")]
    pub descriptor: String,
    /// Wallet change descriptor
    #[clap(env = "CHANGE_DESCRIPTOR")]
    pub change_descriptor: Option<String>,
    /// Earliest block height to start sync from
    #[clap(env = "START_HEIGHT", long, default_value = "0")]
    pub start_height: u32,
    /// Bitcoin network to connect to
    #[clap(env = "BITCOIN_NETWORK", long, default_value = "regtest")]
    pub network: Network,
    /// Where to store wallet data
    #[clap(
        env = "BDK_DB_PATH",
        long,
        default_value = "test_data/bdk-example-rpc.sqlite"
    )]
    pub db_path: PathBuf,

    /// RPC URL
    #[clap(env = "RPC_URL", long, default_value = "127.0.0.1:18443")]
    pub url: String,
    /// RPC auth cookie file
    #[clap(
        env = "RPC_COOKIE",
        long,
        default_value = "test_data/bitcoind/regtest/.cookie"
    )]
    pub rpc_cookie: Option<PathBuf>,
    /// RPC auth username
    #[clap(env = "RPC_USER", long)]
    pub rpc_user: Option<String>,
    /// RPC auth password
    #[clap(env = "RPC_PASS", long)]
    pub rpc_pass: Option<String>,
}

impl Args {
    fn client(&self) -> anyhow::Result<Client> {
        Ok(Client::new(
            &self.url,
            match (&self.rpc_cookie, &self.rpc_user, &self.rpc_pass) {
                (None, None, None) => Auth::None,
                (Some(path), _, _) => Auth::CookieFile(path.clone()),
                (_, Some(user), Some(pass)) => Auth::UserPass(user.clone(), pass.clone()),
                (_, Some(_), None) => panic!("rpc auth: missing rpc_pass"),
                (_, None, Some(_)) => panic!("rpc auth: missing rpc_user"),
            },
        )?)
    }
}

#[derive(Debug)]
enum Emission {
    SigTerm,
    Block(bdk_bitcoind_rpc::BlockEvent<Block>),
    Mempool(MempoolEvent),
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let rpc_client = Arc::new(args.client()?);
    println!(
        "Connected to Bitcoin Core RPC at {:?}",
        rpc_client.get_blockchain_info().unwrap()
    );

    let start_load_wallet = Instant::now();
    let mut db = Connection::open(args.db_path)?;

    let mut params = LoadParams::new()
        .check_descriptor(KeychainKind::External, Some(args.descriptor.clone()))
        .check_genesis_hash(bitcoin::constants::genesis_block(args.network).block_hash())
        .check_network(args.network);

    if let Some(desc) = &args.change_descriptor {
        params = params.check_descriptor(KeychainKind::Internal, Some(desc.clone()));
    }

    let mut wallet = match params.load_wallet(&mut db).unwrap() {
        Some(wallet) => wallet,
        None => {
            let mut keyring: KeyRing<KeychainKind> = KeyRing::new(
                args.network,
                KeychainKind::External,
                args.descriptor.clone(),
            )
            .unwrap();
            if let Some(desc) = &args.change_descriptor {
                keyring
                    .add_descriptor(KeychainKind::Internal, desc.clone())
                    .unwrap();
            }
            Wallet::create(keyring).create_wallet(&mut db)?
        }
    };

    println!(
        "Loaded wallet in {}s",
        start_load_wallet.elapsed().as_secs_f32()
    );

    let address = wallet
        .next_unused_address(KeychainKind::External)
        .unwrap()
        .address;
    println!("Wallet address: {address}");

    let balance = wallet.balance();
    println!("Wallet balance before syncing: {}", balance.total());

    let wallet_tip = wallet.latest_checkpoint();
    println!(
        "Wallet tip: {} at height {}",
        wallet_tip.hash(),
        wallet_tip.height()
    );

    let (sender, receiver) = sync_channel::<Emission>(21);

    let signal_sender = sender.clone();
    let _ = ctrlc::set_handler(move || {
        signal_sender
            .send(Emission::SigTerm)
            .expect("failed to send sigterm")
    });

    let mut emitter = Emitter::new(
        rpc_client,
        wallet_tip,
        args.start_height,
        wallet
            .transactions()
            .filter(|tx| tx.chain_position.is_unconfirmed()),
    );
    spawn(move || -> Result<(), anyhow::Error> {
        while let Some(emission) = emitter.next_block()? {
            sender.send(Emission::Block(emission))?;
        }
        sender.send(Emission::Mempool(emitter.mempool()?))?;
        Ok(())
    });

    let mut blocks_received = 0_usize;
    for emission in receiver {
        match emission {
            Emission::SigTerm => {
                println!("Sigterm received, exiting...");
                break;
            }
            Emission::Block(block_emission) => {
                blocks_received += 1;
                let height = block_emission.block_height();
                let hash = block_emission.block_hash();
                let connected_to = block_emission.connected_to();
                let start_apply_block = Instant::now();
                wallet.apply_block_connected_to(&block_emission.block, height, connected_to)?;
                wallet.persist(&mut db)?;
                let elapsed = start_apply_block.elapsed().as_secs_f32();
                println!("Applied block {hash} at height {height} in {elapsed}s");
            }
            Emission::Mempool(event) => {
                let start_apply_mempool = Instant::now();
                wallet.apply_evicted_txs(event.evicted);
                wallet.apply_unconfirmed_txs(event.update);
                wallet.persist(&mut db)?;
                println!(
                    "Applied unconfirmed transactions in {}s",
                    start_apply_mempool.elapsed().as_secs_f32()
                );
                break;
            }
        }
    }
    let wallet_tip_end = wallet.latest_checkpoint();
    let balance = wallet.balance();
    println!(
        "Synced {} blocks in {}s",
        blocks_received,
        start_load_wallet.elapsed().as_secs_f32(),
    );
    println!(
        "Wallet tip is '{}:{}'",
        wallet_tip_end.height(),
        wallet_tip_end.hash()
    );
    println!("Wallet balance is {}", balance.total());
    println!(
        "Wallet has {} transactions and {} utxos",
        wallet.transactions().count(),
        wallet.list_unspent().count()
    );

    Ok(())
}

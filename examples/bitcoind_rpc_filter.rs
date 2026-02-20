#![allow(unused)]

use bdk_chain::BlockId;
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{
    bitcoin::{Block, Network},
    KeychainKind, Wallet,
};
use bitcoin::{hashes::Hash, BlockHash};
use clap::{self, Parser};
use std::{
    path::PathBuf,
    sync::{mpsc::sync_channel, Arc},
    thread::spawn,
    time::Instant,
};

fn main() -> anyhow::Result<()> {
    Ok(())
}

use std::{str::FromStr, sync::Arc, time::Duration};

use anyhow::Context;
use bdk_testenv::{bitcoincore_rpc::RpcApi, TestEnv};
use bdk_wallet::{KeychainKind, SignOptions, Wallet};
use bitcoin::{Amount, Network, Txid};

const DESCRIPTOR: &str = bdk_testenv::utils::DESCRIPTORS[3];

fn main() -> anyhow::Result<()> {
    let env = TestEnv::new().context("failed to start testenv")?;
    env.mine_blocks(101, None)
        .context("failed to mine blocks")?;

    let mut wallet = Wallet::create_single(DESCRIPTOR)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .context("failed to construct wallet")?;

    let mut emitter = bdk_bitcoind_rpc::Emitter::new(
        env.rpc_client(),
        wallet.latest_checkpoint(),
        0,
        wallet
            .transactions()
            .filter(|tx| tx.chain_position.is_unconfirmed())
            .map(|tx| tx.tx_node.txid),
    );
    while let Some(block_event) = emitter.next_block()? {
        wallet.apply_block(&block_event.block, block_event.block_height())?;
    }

    // Receive an unconfirmed tx, spend from it, and the unconfirmed tx get's RBF'ed.
    // Our API should be able to recognise that the outgoing tx became evicted and allow the caller
    // to respond accordingly.
    let wallet_addr = wallet.next_unused_address(KeychainKind::External).address;
    let remote_addr = env
        .rpc_client()
        .get_new_address(None, None)?
        .assume_checked();
    let incoming_txid = env.send(&wallet_addr, Amount::ONE_BTC)?;

    let mempool_event = emitter.mempool()?;
    wallet.apply_evicted_txs(mempool_event.evicted_ats());
    wallet.apply_unconfirmed_txs(mempool_event.new_txs);
    assert_eq!(wallet.balance().total(), Amount::ONE_BTC);

    // Create & broadcast outgoing tx.
    let mut tx_builder = wallet.build_tx();
    tx_builder.add_recipient(remote_addr, Amount::ONE_BTC / 2);
    let mut psbt = tx_builder.finish()?;
    assert!(wallet.sign(&mut psbt, SignOptions::default())?);
    let outgoing_tx = psbt.extract_tx()?;
    wallet.track_tx(outgoing_tx.clone());
    assert_eq!(wallet.uncanonical_txs().count(), 1);

    // Sync.
    let outgoing_txid = env.rpc_client().send_raw_transaction(&outgoing_tx)?;
    env.wait_until_electrum_sees_txid(outgoing_txid, Duration::from_secs(5))?;
    let mempool_event = emitter.mempool()?;
    // TODO: Why is `outgoing_txid` not emitted?
    println!("mempool_event: {mempool_event:#?}");
    wallet.apply_evicted_txs(mempool_event.evicted_ats());
    wallet.apply_unconfirmed_txs(mempool_event.new_txs);
    let tx = wallet
        .canonical_txs()
        .find(|tx| tx.tx_node.txid == outgoing_txid)
        .expect("must find outgoing tx");
    assert_eq!(wallet.uncanonical_txs().count(), 0);

    // RBF incoming tx.
    let res = env
        .rpc_client()
        .call::<serde_json::Value>("bumpfee", &[incoming_txid.to_string().into()])?;
    let incoming_replacement_txid = Txid::from_str(res.get("txid").unwrap().as_str().unwrap())?;

    let mempool_event = emitter.mempool()?;
    wallet.apply_evicted_txs(mempool_event.evicted_ats());
    wallet.apply_unconfirmed_txs(mempool_event.new_txs);

    for uncanonical_tx in wallet.uncanonical_txs() {}

    Ok(())
}

use std::{ops::Deref, str::FromStr};

use anyhow::Context;
use bdk_bitcoind_rpc::Emitter;
use bdk_testenv::{bitcoincore_rpc::RpcApi, TestEnv};
use bdk_wallet::{KeychainKind, SignOptions, Wallet};
use bitcoin::{Amount, Network, Txid};

const DESCRIPTOR: &str = bdk_testenv::utils::DESCRIPTORS[3];

fn sync_to_tip<C>(wallet: &mut Wallet, emitter: &mut Emitter<C>) -> anyhow::Result<()>
where
    C: Deref,
    C::Target: RpcApi,
{
    while let Some(block_event) = emitter.next_block()? {
        wallet.apply_block(&block_event.block, block_event.block_height())?;
    }
    Ok(())
}

fn sync_mempool<C>(wallet: &mut Wallet, emitter: &mut Emitter<C>) -> anyhow::Result<()>
where
    C: Deref,
    C::Target: RpcApi,
{
    let event = emitter.mempool()?;
    wallet.apply_unconfirmed_txs(event.update);
    wallet.apply_evicted_txs(event.evicted);
    Ok(())
}

/// Receive an unconfirmed tx, spend from it, and the unconfirmed tx get's RBF'ed.
/// Our API should be able to recognise that the outgoing tx became evicted and allow the caller
/// to respond accordingly.
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
            .filter(|tx| tx.chain_position.is_unconfirmed()),
    );

    let wallet_addr = wallet.next_unused_address(KeychainKind::External).address;
    let remote_addr = env
        .rpc_client()
        .get_new_address(None, None)?
        .require_network(Network::Regtest)?;

    sync_to_tip(&mut wallet, &mut emitter)?;

    // [INCOMING TX] : Create, broadcast & sync
    let incoming_txid = env.send(&wallet_addr, Amount::ONE_BTC)?;
    sync_mempool(&mut wallet, &mut emitter)?;
    assert_eq!(wallet.balance().total(), Amount::ONE_BTC);

    // [OUTGOING TX] : Create & track
    let outgoing_tx = {
        let mut tx_builder = wallet.build_tx();
        tx_builder.add_recipient(remote_addr, Amount::ONE_BTC / 2);
        let mut psbt = tx_builder.finish()?;
        assert!(wallet.sign(&mut psbt, SignOptions::default())?);
        psbt.extract_tx()?
    };
    wallet.track_tx(outgoing_tx.clone());
    assert_eq!(wallet.uncanonical_txs().count(), 1);

    // let outgoing_txid = env.rpc_client().send_raw_transaction(&outgoing_tx)?;
    // env.wait_until_electrum_sees_txid(outgoing_txid, Duration::from_secs(5))?;
    // let mempool_event = emitter.mempool()?;
    // println!("mempool_event: {mempool_event:?}");
    // wallet.apply_unconfirmed_txs(mempool_event.update);
    // wallet.apply_evicted_txs(mempool_event.evicted);
    // let tx = wallet
    //     .canonical_txs()
    //     .find(|tx| tx.tx_node.txid == outgoing_txid)
    //     .expect("must find outgoing tx");
    // assert_eq!(wallet.uncanonical_txs().count(), 0);

    // RBF incoming tx.
    let incoming_rbf_tx = {
        let res = env
            .rpc_client()
            .call::<serde_json::Value>("bumpfee", &[incoming_txid.to_string().into()])?;
        Txid::from_str(res.get("txid").unwrap().as_str().unwrap())?
    };
    sync_mempool(&mut wallet, &mut emitter)?;

    for uncanonical_tx in wallet.uncanonical_txs() {}

    Ok(())
}

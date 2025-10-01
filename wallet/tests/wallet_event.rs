#![allow(deprecated)]
use bdk_chain::{BlockId, CheckPoint, ConfirmationBlockTime};
use bdk_wallet::event::WalletEvent;
use bdk_wallet::test_utils::{get_test_wpkh_and_change_desc, new_wallet_and_funding_update};
use bdk_wallet::{SignOptions, Update};
use bitcoin::hashes::Hash;
use bitcoin::{Address, Amount, BlockHash, FeeRate};
use core::str::FromStr;
use std::sync::Arc;

#[test]
fn test_new_confirmed_tx_event() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _, update) = new_wallet_and_funding_update(desc, Some(change_desc));

    let genesis = BlockId {
        height: 0,
        hash: wallet.local_chain().genesis_hash(),
    };
    let events = wallet.apply_update_events(update).unwrap();
    let new_tip1 = wallet.local_chain().tip().block_id();
    assert_eq!(events.len(), 3);
    assert!(
        matches!(events[0], WalletEvent::ChainTipChanged { old_tip, new_tip } if old_tip == genesis && new_tip == new_tip1)
    );
    assert!(
        matches!(events[1], WalletEvent::TxConfirmed { block_time, ..} if block_time.block_id.height == 1000)
    );
    assert!(matches!(&events[1], WalletEvent::TxConfirmed {tx, ..} if tx.output.len() == 1));
    assert!(
        matches!(events[2], WalletEvent::TxConfirmed {block_time, ..} if block_time.block_id.height == 2000)
    );
    assert!(matches!(&events[2], WalletEvent::TxConfirmed {tx, ..} if tx.output.len() == 2));
}

#[test]
fn test_tx_unconfirmed_event() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _, update) = new_wallet_and_funding_update(desc, Some(change_desc));
    // ignore funding events
    let _events = wallet.apply_update_events(update).unwrap();

    let reorg_block = BlockId {
        height: 2_000,
        hash: BlockHash::from_slice(&[1; 32]).unwrap(),
    };
    let mut cp = wallet.latest_checkpoint();
    cp = cp.insert(reorg_block);
    let reorg_update = Update {
        chain: Some(cp),
        ..Default::default()
    };
    let old_tip1 = wallet.local_chain().tip().block_id();
    let events = wallet.apply_update_events(reorg_update).unwrap();
    let new_tip1 = wallet.local_chain().tip().block_id();
    assert_eq!(events.len(), 2);
    assert!(
        matches!(events[0], WalletEvent::ChainTipChanged { old_tip, new_tip } if old_tip == old_tip1 && new_tip == new_tip1)
    );
    assert!(
        matches!(&events[1], WalletEvent::TxUnconfirmed {tx, old_block_time, ..} if tx.output.len() == 2 && old_block_time.is_some())
    );
}

#[test]
fn test_tx_replaced_event() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _, update) = new_wallet_and_funding_update(desc, Some(change_desc));
    // ignore funding events
    let _events = wallet.apply_update_events(update).unwrap();

    // create original tx
    let mut builder = wallet.build_tx();
    builder.add_recipient(
        Address::from_str("tb1q6yn66vajcctph75pvylgkksgpp6nq04ppwct9a")
            .unwrap()
            .assume_checked(),
        Amount::from_sat(10_000),
    );
    let mut psbt = builder.finish().unwrap();
    wallet.sign(&mut psbt, SignOptions::default()).unwrap();
    let orig_tx = Arc::new(psbt.extract_tx().unwrap());
    let orig_txid = orig_tx.compute_txid();

    // update wallet with original tx
    let mut update = Update::default();
    update.tx_update.txs = vec![orig_tx.clone()];
    update.tx_update.seen_ats = [(orig_txid, 210)].into();
    let events = wallet.apply_update_events(update).unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], WalletEvent::TxUnconfirmed { .. }));
    assert!(
        matches!(&events[0], WalletEvent::TxUnconfirmed {tx, ..} if tx.compute_txid() == orig_txid)
    );

    // create rbf tx
    let mut builder = wallet.build_fee_bump(orig_txid).unwrap();
    builder.fee_rate(FeeRate::from_sat_per_vb(10).unwrap());
    let mut psbt = builder.finish().unwrap();
    wallet.sign(&mut psbt, SignOptions::default()).unwrap();
    let rbf_tx = Arc::new(psbt.extract_tx().unwrap());
    let rbf_txid = rbf_tx.compute_txid();

    // update wallet with rbf tx
    let mut update = Update::default();
    update.tx_update.txs = vec![rbf_tx.clone()];
    update.tx_update.evicted_ats = [(orig_txid, 220)].into();
    update.tx_update.seen_ats = [(rbf_txid, 220)].into();

    let events = wallet.apply_update_events(update).unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], WalletEvent::TxUnconfirmed { txid, .. } if txid == rbf_txid));
    assert!(matches!(events[1], WalletEvent::TxReplaced { txid, ..} if txid == orig_txid));
    assert!(
        matches!(&events[1], WalletEvent::TxReplaced {conflicts, ..} if conflicts.len() == 1 &&
            conflicts.contains(&(0, rbf_txid)))
    );
}

#[test]
fn test_tx_confirmed_event() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _, update) = new_wallet_and_funding_update(desc, Some(change_desc));
    // ignore funding events
    let _events = wallet.apply_update_events(update).unwrap();

    // create new tx
    let mut builder = wallet.build_tx();
    builder.add_recipient(
        Address::from_str("tb1q6yn66vajcctph75pvylgkksgpp6nq04ppwct9a")
            .unwrap()
            .assume_checked(),
        Amount::from_sat(10_000),
    );
    let mut psbt = builder.finish().unwrap();
    wallet.sign(&mut psbt, SignOptions::default()).unwrap();
    let new_tx = Arc::new(psbt.extract_tx().unwrap());
    let new_txid = new_tx.compute_txid();

    // update wallet with original tx
    let mut update = Update::default();
    update.tx_update.txs = vec![new_tx.clone()];
    update.tx_update.seen_ats = [(new_txid, 210)].into();
    let events = wallet.apply_update_events(update).unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], WalletEvent::TxUnconfirmed { .. }));
    assert!(
        matches!(&events[0], WalletEvent::TxUnconfirmed {tx, ..} if tx.compute_txid() == new_txid)
    );

    // confirm tx
    let mut update = Update::default();
    let parent_block = BlockId {
        height: 2000,
        hash: BlockHash::all_zeros(),
    };
    let new_block = BlockId {
        height: 2100,
        hash: BlockHash::all_zeros(),
    };

    let new_anchor = ConfirmationBlockTime {
        block_id: new_block,
        confirmation_time: 300,
    };
    update.chain = CheckPoint::from_block_ids([parent_block, new_block]).ok();
    update.tx_update.anchors = [(new_anchor, new_txid)].into();

    let orig_tip = wallet.local_chain().tip().block_id();
    let events = wallet.apply_update_events(update).unwrap();
    assert_eq!(events.len(), 2);
    assert!(
        matches!(events[0], WalletEvent::ChainTipChanged { old_tip, new_tip } if old_tip == orig_tip && new_tip == new_block)
    );
    assert!(matches!(events[1], WalletEvent::TxConfirmed { txid, .. } if txid == new_txid));
}

#[test]
fn test_tx_confirmed_new_block_event() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _, update) = new_wallet_and_funding_update(desc, Some(change_desc));
    // ignore funding events
    let _events = wallet.apply_update_events(update).unwrap();

    // create new tx
    let mut builder = wallet.build_tx();
    builder.add_recipient(
        Address::from_str("tb1q6yn66vajcctph75pvylgkksgpp6nq04ppwct9a")
            .unwrap()
            .assume_checked(),
        Amount::from_sat(10_000),
    );
    let mut psbt = builder.finish().unwrap();
    wallet.sign(&mut psbt, SignOptions::default()).unwrap();
    let new_tx = Arc::new(psbt.extract_tx().unwrap());
    let new_txid = new_tx.compute_txid();

    // update wallet with original tx
    let mut update = Update::default();
    update.tx_update.txs = vec![new_tx.clone()];
    update.tx_update.seen_ats = [(new_txid, 210)].into();
    let events = wallet.apply_update_events(update).unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], WalletEvent::TxUnconfirmed { .. }));
    assert!(
        matches!(&events[0], WalletEvent::TxUnconfirmed {tx, ..} if tx.compute_txid() == new_txid)
    );

    // confirm tx
    let mut update = Update::default();
    let parent_block = BlockId {
        height: 2000,
        hash: BlockHash::all_zeros(),
    };
    let new_block = BlockId {
        height: 2100,
        hash: BlockHash::all_zeros(),
    };

    let new_anchor = ConfirmationBlockTime {
        block_id: new_block,
        confirmation_time: 300,
    };
    update.chain = CheckPoint::from_block_ids([parent_block, new_block]).ok();
    update.tx_update.anchors = [(new_anchor, new_txid)].into();

    let orig_tip = wallet.local_chain().tip().block_id();
    let events = wallet.apply_update_events(update).unwrap();
    assert_eq!(events.len(), 2);
    assert!(
        matches!(events[0], WalletEvent::ChainTipChanged { old_tip, new_tip } if old_tip == orig_tip && new_tip == new_block)
    );
    assert!(matches!(events[1], WalletEvent::TxConfirmed { txid, .. } if txid == new_txid));

    // confirm reorged tx
    let mut update = Update::default();
    let parent_block = BlockId {
        height: 2000,
        hash: BlockHash::all_zeros(),
    };
    let reorg_block = BlockId {
        height: 2100,
        hash: BlockHash::from_slice(&[1; 32]).unwrap(),
    };

    let reorg_anchor = ConfirmationBlockTime {
        block_id: reorg_block,
        confirmation_time: 310,
    };
    update.chain = CheckPoint::from_block_ids([parent_block, reorg_block]).ok();
    update.tx_update.anchors = [(reorg_anchor, new_txid)].into();

    let events = wallet.apply_update_events(update).unwrap();
    assert_eq!(events.len(), 2);
    assert!(
        matches!(events[0], WalletEvent::ChainTipChanged { old_tip, new_tip } if old_tip == new_block && new_tip == reorg_block)
    );
    assert!(
        matches!(events[1], WalletEvent::TxConfirmed { txid, block_time, old_block_time, .. } if txid == new_txid && block_time.block_id == reorg_block && old_block_time.is_some())
    );
}

#[test]
fn test_tx_dropped_event() {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    let (mut wallet, _, update) = new_wallet_and_funding_update(desc, Some(change_desc));
    // ignore funding events
    let _events = wallet.apply_update_events(update).unwrap();

    // create new tx
    let mut builder = wallet.build_tx();
    builder.add_recipient(
        Address::from_str("tb1q6yn66vajcctph75pvylgkksgpp6nq04ppwct9a")
            .unwrap()
            .assume_checked(),
        Amount::from_sat(10_000),
    );
    let mut psbt = builder.finish().unwrap();
    wallet.sign(&mut psbt, SignOptions::default()).unwrap();
    let new_tx = Arc::new(psbt.extract_tx().unwrap());
    let new_txid = new_tx.compute_txid();

    // update wallet with original tx
    let mut update = Update::default();
    update.tx_update.txs = vec![new_tx.clone()];
    update.tx_update.seen_ats = [(new_txid, 210)].into();
    let events = wallet.apply_update_events(update).unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], WalletEvent::TxUnconfirmed { .. }));
    assert!(
        matches!(&events[0], WalletEvent::TxUnconfirmed {tx, ..} if tx.compute_txid() == new_txid)
    );

    // drop tx
    let mut update = Update::default();
    update.tx_update.evicted_ats = [(new_txid, 220)].into();
    let events = wallet.apply_update_events(update).unwrap();

    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], WalletEvent::TxDropped { txid, .. } if txid == new_txid));
}

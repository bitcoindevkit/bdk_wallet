use bdk_chain::{ConfirmationBlockTime, DescriptorExt, SpkIterator};
use bdk_wallet::persist_test_utils::*;
use bdk_wallet::ChangeSet;
use bitcoin::{Amount, Network, OutPoint, TxOut};
use miniscript::{Descriptor, DescriptorPublicKey};
use std::sync::Arc;

mod common;

// What this test validates:
// - v3 can deserialize v2 JSON (backwards compat)
// - v2 can deserialize v3 JSON and ignore new fields (forwards compat)
// - New fields added in v3 implement Default correctly
// - For simplicity JSON is chosen as the serialization format
#[test]
fn test_changeset_compatibility_v2_to_v3() {
    let v2_change_set = get_changeset_v2();
    let v2_json = serde_json::to_string(&v2_change_set).expect("failed to serialize v2_change_set");

    // Test deserialize v2_change_set with the current version (backwards compatibility)
    let v3_change_set: ChangeSet =
        serde_json::from_str(&v2_json).expect("failed to deserialize v2_change_set");

    // v3 added locked_outpoints - verify Default was applied
    assert!(
        v3_change_set.locked_outpoints.outpoints.is_empty(),
        "Failed to populate new default field `locked_outpoints`"
    );

    let v3_change_set = get_changeset_v3();
    assert!(!v3_change_set.locked_outpoints.outpoints.is_empty());
    let v3_json = serde_json::to_string(&v3_change_set).expect("failed to serialize v3_change_set");

    // v2 should ignore unknown fields when reading v3 data
    let _: bdk_wallet_2_3_0::ChangeSet =
        serde_json::from_str(&v3_json).expect("failed to deserialize v3_change_set");
}

#[test]
fn test_changeset_v2_roundtrip_through_v3() {
    // Ensure v2 data survives a write/read cycle through v3 code
    let v2_change_set = get_changeset_v2();
    let v2_json = serde_json::to_string(&v2_change_set).unwrap();

    // Deserialize into v3
    let v3_change_set: ChangeSet = serde_json::from_str(&v2_json).unwrap();

    // Re-serialize from v3
    let v3_json = serde_json::to_string(&v3_change_set).unwrap();

    // Deserialize back into v2 - should still work
    let v2_roundtrip: bdk_wallet_2_3_0::ChangeSet = serde_json::from_str(&v3_json)
        .expect("v2 must still deserialize after roundtrip through v3");

    // Verify data is preserved
    assert_eq!(v2_roundtrip, v2_change_set);
}

/// Get v3 change set.
pub fn get_changeset_v3() -> ChangeSet {
    let change_set = get_changeset_v2();

    ChangeSet {
        descriptor: change_set.descriptor,
        change_descriptor: change_set.change_descriptor,
        network: change_set.network,
        local_chain: change_set.local_chain,
        tx_graph: change_set.tx_graph,
        indexer: change_set.indexer,
        locked_outpoints: bdk_wallet::locked_outpoints::ChangeSet {
            outpoints: [(OutPoint::new(hash!("Rust"), 0), true)].into(),
        },
    }
}

/// Get v2 change set.
pub fn get_changeset_v2() -> bdk_wallet_2_3_0::ChangeSet {
    use bdk_wallet_2_3_0::chain::{keychain_txout, local_chain, tx_graph};
    use bdk_wallet_2_3_0::ChangeSet;

    let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();
    let change_descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[1].parse().unwrap();

    let local_chain_changeset = local_chain::ChangeSet {
        blocks: [
            (0, Some(hash!("0"))),
            (910233, Some(hash!("B"))),
            (910234, Some(hash!("T"))),
            (910235, Some(hash!("C"))),
        ]
        .into(),
    };

    let tx = Arc::new(create_one_inp_one_out_tx(hash!("prev_txid"), 30_000));

    let txid = tx.compute_txid();

    let conf_anchor: ConfirmationBlockTime = ConfirmationBlockTime {
        block_id: block_id!(910233, "B"),
        confirmation_time: 1755317160,
    };

    let tx_graph_changeset = tx_graph::ChangeSet {
        txs: [tx].into(),
        txouts: [
            (
                OutPoint::new(hash!("Rust"), 0),
                TxOut {
                    value: Amount::from_sat(1300),
                    script_pubkey: spk_at_index(&descriptor, 4),
                },
            ),
            (
                OutPoint::new(hash!("REDB"), 0),
                TxOut {
                    value: Amount::from_sat(1400),
                    script_pubkey: spk_at_index(&descriptor, 10),
                },
            ),
        ]
        .into(),
        anchors: [(conf_anchor, txid)].into(),
        last_seen: [(txid, 1755317760)].into(),
        first_seen: [(txid, 1755317750)].into(),
        last_evicted: [(txid, 1755317760)].into(),
    };

    let keychain_txout_changeset = keychain_txout::ChangeSet {
        last_revealed: [
            (descriptor.descriptor_id(), 3),
            (change_descriptor.descriptor_id(), 5),
        ]
        .into(),
        spk_cache: [
            (
                descriptor.descriptor_id(),
                SpkIterator::new_with_range(&descriptor, 0..=10).collect(),
            ),
            (
                change_descriptor.descriptor_id(),
                SpkIterator::new_with_range(&change_descriptor, 0..=10).collect(),
            ),
        ]
        .into(),
    };

    ChangeSet {
        descriptor: Some(descriptor),
        change_descriptor: Some(change_descriptor),
        network: Some(Network::Regtest),
        local_chain: local_chain_changeset,
        tx_graph: tx_graph_changeset,
        indexer: keychain_txout_changeset,
    }
}

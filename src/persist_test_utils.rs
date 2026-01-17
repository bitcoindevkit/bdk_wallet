//! Utilities for testing custom persistence backends for `bdk_wallet`
#![allow(unused)]
use crate::{
    bitcoin::{
        absolute, key::Secp256k1, transaction, Address, Amount, Network, OutPoint, ScriptBuf,
        Transaction, TxIn, TxOut, Txid,
    },
    chain::{
        keychain_txout::{self},
        local_chain, tx_graph, ConfirmationBlockTime, DescriptorExt, Merge, SpkIterator,
    },
    keyring, locked_outpoints,
    miniscript::descriptor::{Descriptor, DescriptorPublicKey},
    ChangeSet, KeychainKind, WalletPersister,
};

macro_rules! block_id {
    ($height:expr, $hash:literal) => {{
        bdk_chain::BlockId {
            height: $height,
            hash: bitcoin::hashes::Hash::hash($hash.as_bytes()),
        }
    }};
}

macro_rules! hash {
    ($index:literal) => {{
        bitcoin::hashes::Hash::hash($index.as_bytes())
    }};
}

use std::fmt;
use std::fmt::Debug;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

const DESCRIPTORS: [&str; 4] = [
    "tr([5940b9b9/86'/0'/0']tpubDDVNqmq75GNPWQ9UNKfP43UwjaHU4GYfoPavojQbfpyfZp2KetWgjGBRRAy4tYCrAA6SB11mhQAkqxjh1VtQHyKwT4oYxpwLaGHvoKmtxZf/0/*)#44aqnlam",
    "tr([5940b9b9/86'/0'/0']tpubDDVNqmq75GNPWQ9UNKfP43UwjaHU4GYfoPavojQbfpyfZp2KetWgjGBRRAy4tYCrAA6SB11mhQAkqxjh1VtQHyKwT4oYxpwLaGHvoKmtxZf/1/*)#ypcpw2dr",
    "wpkh([41f2aed0/84h/1h/0h]tpubDDFSdQWw75hk1ewbwnNpPp5DvXFRKt68ioPoyJDY752cNHKkFxPWqkqCyCf4hxrEfpuxh46QisehL3m8Bi6MsAv394QVLopwbtfvryFQNUH/0/*)#g0w0ymmw",
    "wpkh([41f2aed0/84h/1h/0h]tpubDDFSdQWw75hk1ewbwnNpPp5DvXFRKt68ioPoyJDY752cNHKkFxPWqkqCyCf4hxrEfpuxh46QisehL3m8Bi6MsAv394QVLopwbtfvryFQNUH/1/*)#emtwewtk",
];

fn create_one_inp_one_out_tx(txid: Txid, amount: u64) -> Transaction {
    Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(txid, 0),
            ..TxIn::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(amount),
            script_pubkey: Address::from_str("bcrt1q3qtze4ys45tgdvguj66zrk4fu6hq3a3v9pfly5")
                .unwrap()
                .assume_checked()
                .script_pubkey(),
        }],
    }
}

fn spk_at_index(descriptor: &Descriptor<DescriptorPublicKey>, index: u32) -> ScriptBuf {
    descriptor
        .derived_descriptor(&Secp256k1::verification_only(), index)
        .expect("must derive")
        .script_pubkey()
}

/// tests if [`Wallet`] is being persisted correctly
///
/// [`Wallet`]: <https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.Wallet.html>
/// [`ChangeSet`]: <https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.ChangeSet.html>
///
/// We create a dummy [`ChangeSet`], persist it and check if loaded [`ChangeSet`] matches
/// the persisted one. We then create another such dummy [`ChangeSet`], persist it and load it to
/// check if merged [`ChangeSet`] is returned.
pub fn persist_wallet_changeset<Store, CreateStore, K>(
    filename: &str,
    create_store: CreateStore,
    keychain: K,
) where
    CreateStore: Fn(&Path) -> anyhow::Result<Store>,
    Store: WalletPersister<K>,
    Store::Error: Debug,
    K: Ord + Clone + fmt::Debug,
{
    // create store
    let temp_dir = tempfile::tempdir().expect("must create tempdir");
    let file_path = temp_dir.path().join(filename);
    let mut store = create_store(&file_path).expect("store should get created");

    // initialize store
    let changeset =
        WalletPersister::initialize(&mut store).expect("empty changeset should get loaded");
    assert_eq!(changeset, ChangeSet::default());

    // create changeset
    let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();

    let local_chain_changeset = local_chain::ChangeSet {
        blocks: [
            (910234, Some(hash!("B"))),
            (910233, Some(hash!("T"))),
            (910235, Some(hash!("C"))),
        ]
        .into(),
    };

    let tx1 = Arc::new(create_one_inp_one_out_tx(
        hash!("We_are_all_Satoshi"),
        30_000,
    ));
    let tx2 = Arc::new(create_one_inp_one_out_tx(tx1.compute_txid(), 20_000));

    let conf_anchor = ConfirmationBlockTime {
        block_id: block_id!(910234, "B"),
        confirmation_time: 1755317160,
    };

    let outpoint = OutPoint::new(hash!("Rust"), 0);

    let tx_graph_changeset = tx_graph::ChangeSet::<ConfirmationBlockTime> {
        txs: [tx1.clone()].into(),
        txouts: [
            (
                outpoint,
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
        anchors: [(conf_anchor, tx1.compute_txid())].into(),
        last_seen: [(tx1.compute_txid(), 1755317760)].into(),
        first_seen: [(tx1.compute_txid(), 1755317750)].into(),
        last_evicted: [(tx1.compute_txid(), 1755317760)].into(),
    };

    let keychain_txout_changeset = keychain_txout::ChangeSet {
        last_revealed: [(descriptor.descriptor_id(), 12)].into(),
        spk_cache: [(
            descriptor.descriptor_id(),
            SpkIterator::new_with_range(&descriptor, 0..=37).collect(),
        )]
        .into(),
    };

    let locked_outpoints_changeset = locked_outpoints::ChangeSet {
        outpoints: [(outpoint, true)].into(),
    };

    let keyring_changeset = crate::keyring::ChangeSet {
        network: Some(Network::Testnet),
        descriptors: [(keychain.clone(), descriptor.clone())].into(),
        default_keychain: Some(keychain),
    };

    let mut changeset = ChangeSet {
        keyring: keyring_changeset,
        local_chain: local_chain_changeset,
        tx_graph: tx_graph_changeset,
        indexer: keychain_txout_changeset,
        locked_outpoints: locked_outpoints_changeset,
    };

    // persist and load
    WalletPersister::persist(&mut store, &changeset).expect("changeset should get persisted");

    let changeset_read =
        WalletPersister::initialize(&mut store).expect("changeset should get loaded");

    assert_eq!(changeset, changeset_read);

    // create another changeset
    let local_chain_changeset = local_chain::ChangeSet {
        blocks: [(910236, Some(hash!("BDK")))].into(),
    };

    let conf_anchor: ConfirmationBlockTime = ConfirmationBlockTime {
        block_id: block_id!(910236, "BDK"),
        confirmation_time: 1755317760,
    };

    let outpoint = OutPoint::new(hash!("Bitcoin_fixes_things"), 1);

    let tx_graph_changeset = tx_graph::ChangeSet::<ConfirmationBlockTime> {
        txs: [tx2.clone()].into(),
        txouts: [(
            outpoint,
            TxOut {
                value: Amount::from_sat(10000),
                script_pubkey: spk_at_index(&descriptor, 21),
            },
        )]
        .into(),
        anchors: [(conf_anchor, tx2.compute_txid())].into(),
        last_seen: [(tx2.compute_txid(), 1755317700)].into(),
        first_seen: [(tx2.compute_txid(), 1755317700)].into(),
        last_evicted: [(tx2.compute_txid(), 1755317760)].into(),
    };

    let keychain_txout_changeset = keychain_txout::ChangeSet {
        last_revealed: [(descriptor.descriptor_id(), 14)].into(),
        spk_cache: [(
            descriptor.descriptor_id(),
            SpkIterator::new_with_range(&descriptor, 37..=39).collect(),
        )]
        .into(),
    };

    let locked_outpoints_changeset = locked_outpoints::ChangeSet {
        outpoints: [(outpoint, true)].into(),
    };

    let changeset_new = ChangeSet {
        keyring: keyring::ChangeSet::default(),
        local_chain: local_chain_changeset,
        tx_graph: tx_graph_changeset,
        indexer: keychain_txout_changeset,
        locked_outpoints: locked_outpoints_changeset,
    };

    // persist, load and check if same as merged
    WalletPersister::persist(&mut store, &changeset_new).expect("changeset should get persisted");

    let changeset_read_new = WalletPersister::initialize(&mut store).unwrap();

    changeset.merge(changeset_new);

    assert_eq!(changeset, changeset_read_new);
}

/// tests if multiple [`Wallet`]s can be persisted in a single file correctly
///
/// [`Wallet`]: <https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.Wallet.html>
/// [`ChangeSet`]: <https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.ChangeSet.html>
///
/// We create a dummy [`ChangeSet`] for first wallet and persist it then we create a dummy
/// [`ChangeSet`] for second wallet and persist that. Finally we load these two [`ChangeSet`]s and
/// check if they were persisted correctly.
pub fn persist_multiple_wallet_changesets<Store, CreateStores, K>(
    filename: &str,
    create_dbs: CreateStores,
    keychain: K,
) where
    CreateStores: Fn(&Path) -> anyhow::Result<(Store, Store)>,
    Store: WalletPersister<K>,
    Store::Error: Debug,
    K: Ord + Clone + fmt::Debug,
{
    // create stores
    let temp_dir = tempfile::tempdir().expect("must create tempdir");
    let file_path = temp_dir.path().join(filename);

    let (mut store_first, mut store_sec) =
        create_dbs(&file_path).expect("store should get created");

    // initialize first store
    let changeset =
        WalletPersister::initialize(&mut store_first).expect("should load empty changeset");
    assert_eq!(changeset, ChangeSet::default());

    // create first changeset
    let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();

    let keyring_changeset = crate::keyring::ChangeSet {
        network: Some(Network::Testnet),
        descriptors: [(keychain.clone(), descriptor.clone())].into(),
        default_keychain: Some(keychain.clone()),
    };

    let changeset1 = ChangeSet {
        keyring: keyring_changeset,
        ..ChangeSet::default()
    };

    // persist first changeset
    WalletPersister::persist(&mut store_first, &changeset1).expect("should persist changeset");

    // initialize second store
    let changeset =
        WalletPersister::initialize(&mut store_sec).expect("should load empty changeset");
    assert_eq!(changeset, ChangeSet::default());

    // create second changeset
    let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[2].parse().unwrap();

    let keyring_changeset2 = crate::keyring::ChangeSet {
        network: Some(Network::Testnet),
        descriptors: [(keychain.clone(), descriptor.clone())].into(),
        default_keychain: Some(keychain),
    };

    let changeset2 = ChangeSet {
        keyring: keyring_changeset2,
        ..ChangeSet::default()
    };

    // persist second changeset
    WalletPersister::persist(&mut store_sec, &changeset2).expect("should persist changeset");

    // load first changeset
    let changeset_read =
        WalletPersister::initialize(&mut store_first).expect("should load persisted changeset1");
    assert_eq!(changeset_read, changeset1);

    // load second changeset
    let changeset_read =
        WalletPersister::initialize(&mut store_sec).expect("should load persisted changeset2");
    assert_eq!(changeset_read, changeset2);
}

/// tests if [`Network`] is being persisted correctly
///
/// [`Network`]: <https://docs.rs/bitcoin/latest/bitcoin/enum.Network.html>
/// [`ChangeSet`]: <https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.ChangeSet.html>
///
/// We create a dummy [`ChangeSet`] with only the `network` field of the `keyring` field populated,
/// persist it and check if loaded [`ChangeSet`] has the same [`Network`] as what we persisted.
pub fn persist_network<Store, CreateStore, K>(filename: &str, create_store: CreateStore)
where
    CreateStore: Fn(&Path) -> anyhow::Result<Store>,
    Store: WalletPersister<K>,
    Store::Error: Debug,
    K: Ord + Clone + fmt::Debug,
{
    // create store
    let temp_dir = tempfile::tempdir().expect("must create tempdir");
    let file_path = temp_dir.path().join(filename);
    let mut store = create_store(&file_path).expect("store should get created");

    // initialize store
    let changeset = WalletPersister::initialize(&mut store)
        .expect("should initialize and load empty changeset");
    assert_eq!(changeset, ChangeSet::default());

    let keyring_changeset = crate::keyring::ChangeSet {
        network: Some(Network::Bitcoin),
        ..crate::keyring::ChangeSet::default()
    };

    // persist the network
    let changeset = ChangeSet {
        keyring: keyring_changeset,
        ..ChangeSet::default()
    };
    WalletPersister::persist(&mut store, &changeset).expect("should persist changeset");

    // read the persisted network
    let changeset_read =
        WalletPersister::initialize(&mut store).expect("should load persisted changeset");

    assert_eq!(changeset_read.keyring.network, Some(Network::Bitcoin));
}

/// tests if the descriptor corresponding to [`Wallet`] is being persisted correctly
///
/// [`ChangeSet`]: <https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.ChangeSet.html>
///
/// We create a dummy [`ChangeSet`] with only the `descriptors` and the `default_keychain`
/// populated, persist it and check if loaded [`ChangeSet`] has the same descriptor
/// and `default_keychain` as what we persisted.
pub fn persist_keychain<Store, CreateStore, K>(
    filename: &str,
    create_store: CreateStore,
    keychain: K,
) where
    CreateStore: Fn(&Path) -> anyhow::Result<Store>,
    Store: WalletPersister<K>,
    Store::Error: Debug,
    K: Ord + Clone + fmt::Debug,
{
    // create store
    let temp_dir = tempfile::tempdir().expect("must create tempdir");
    let file_path = temp_dir.path().join(filename);
    let mut store = create_store(&file_path).expect("store should get created");

    // initialize store
    let changeset = WalletPersister::initialize(&mut store)
        .expect("should initialize and load empty changeset");
    assert_eq!(changeset, ChangeSet::default());

    // persist the descriptors
    let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[1].parse().unwrap();

    let keyring_changeset = crate::keyring::ChangeSet {
        descriptors: [(keychain.clone(), descriptor.clone())].into(),
        default_keychain: Some(keychain.clone()),
        ..crate::keyring::ChangeSet::default()
    };

    let changeset = ChangeSet {
        keyring: keyring_changeset,
        ..ChangeSet::default()
    };

    WalletPersister::persist(&mut store, &changeset).expect("should persist descriptors");

    // load the descriptors
    let changeset_read =
        WalletPersister::initialize(&mut store).expect("should read persisted changeset");

    assert_eq!(
        *changeset_read.keyring.descriptors.get(&keychain).unwrap(),
        descriptor
    );

    assert_eq!(changeset_read.keyring.default_keychain, Some(keychain));
}

/// tests if multiple descriptors are being persisted correctly
///
/// [`ChangeSet`]: <https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.ChangeSet.html>
///
/// We create a dummy [`ChangeSet`] with only the `descriptors` and the `default_keychain`
/// populated, persist it and check if loaded [`ChangeSet`] has the same descriptors
/// and `default_keychain` as what we persisted. We then create another such [`ChangeSet`], persist,
/// load and check that the loaded [`ChangeSet`] is same as the merged one.
pub fn persist_keychains<Store, CreateStore, K>(
    filename: &str,
    create_store: CreateStore,
    keychain1: K,
    keychain2: K,
) where
    CreateStore: Fn(&Path) -> anyhow::Result<Store>,
    Store: WalletPersister<K>,
    Store::Error: Debug,
    K: Ord + Clone + fmt::Debug,
{
    // create store
    let temp_dir = tempfile::tempdir().expect("must create tempdir");
    let file_path = temp_dir.path().join(filename);
    let mut store = create_store(&file_path).expect("store should get created");

    // initialize store
    let changeset = WalletPersister::initialize(&mut store)
        .expect("should initialize and load empty changeset");
    assert_eq!(changeset, ChangeSet::default());

    // persist the descriptors
    let desc1: Descriptor<DescriptorPublicKey> = DESCRIPTORS[1].parse().unwrap();
    let desc2: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();

    let keyring_changeset = crate::keyring::ChangeSet {
        descriptors: [
            (keychain1.clone(), desc1.clone()),
            (keychain2.clone(), desc2.clone()),
        ]
        .into(),
        default_keychain: Some(keychain1.clone()),
        ..crate::keyring::ChangeSet::default()
    };

    let changeset = ChangeSet {
        keyring: keyring_changeset,
        ..ChangeSet::default()
    };

    WalletPersister::persist(&mut store, &changeset).expect("should persist descriptors");

    // load the descriptors
    let changeset_read =
        WalletPersister::initialize(&mut store).expect("should read persisted changeset");

    assert_eq!(
        *changeset_read.keyring.descriptors.get(&keychain1).unwrap(),
        desc1
    );
    assert_eq!(
        *changeset_read.keyring.descriptors.get(&keychain2).unwrap(),
        desc2
    );

    assert_eq!(
        changeset_read.keyring.default_keychain,
        Some(keychain1.clone())
    );

    let keyring_changeset_new = crate::keyring::ChangeSet {
        default_keychain: Some(keychain2.clone()),
        ..crate::keyring::ChangeSet::default()
    };

    let changeset_new = ChangeSet {
        keyring: keyring_changeset_new,
        ..ChangeSet::default()
    };

    WalletPersister::persist(&mut store, &changeset_new).expect("should persist descriptors");

    let changeset_read_new =
        WalletPersister::initialize(&mut store).expect("should read persisted changeset");
    assert_eq!(
        changeset_read_new.keyring.default_keychain,
        Some(keychain2.clone())
    );
    assert_eq!(
        *changeset_read_new
            .keyring
            .descriptors
            .get(&keychain1)
            .unwrap(),
        desc1
    );
    assert_eq!(
        *changeset_read_new
            .keyring
            .descriptors
            .get(&keychain2)
            .unwrap(),
        desc2
    );
}

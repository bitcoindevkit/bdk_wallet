//! Utilities for testing custom persistence backends for `bdk_wallet`

use alloc::boxed::Box;
use core::fmt;
#[cfg(feature = "std")]
use std::error::Error as StdErr;

use crate::{
    bitcoin::{
        absolute, key::Secp256k1, transaction, Address, Amount, Network, OutPoint, ScriptBuf,
        Transaction, TxIn, TxOut, Txid,
    },
    chain::{
        keychain_txout::{self},
        local_chain, tx_graph, ConfirmationBlockTime, DescriptorExt, Merge, SpkIterator,
    },
    locked_outpoints,
    miniscript::descriptor::{Descriptor, DescriptorPublicKey},
    AsyncWalletPersister, ChangeSet, WalletPersister,
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
pub fn persist_wallet_changeset<Store, CreateStore>(filename: &str, create_store: CreateStore)
where
    CreateStore: Fn(&Path) -> anyhow::Result<Store>,
    Store: WalletPersister,
    Store::Error: Debug,
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
    let change_descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[1].parse().unwrap();

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

    let conf_anchor: ConfirmationBlockTime = ConfirmationBlockTime {
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
        last_revealed: [
            (descriptor.descriptor_id(), 12),
            (change_descriptor.descriptor_id(), 10),
        ]
        .into(),
        spk_cache: [
            (
                descriptor.descriptor_id(),
                SpkIterator::new_with_range(&descriptor, 0..=37).collect(),
            ),
            (
                change_descriptor.descriptor_id(),
                SpkIterator::new_with_range(&change_descriptor, 0..=35).collect(),
            ),
        ]
        .into(),
    };

    let locked_outpoints_changeset = locked_outpoints::ChangeSet {
        outpoints: [(outpoint, true)].into(),
    };

    let mut changeset = ChangeSet {
        descriptor: Some(descriptor.clone()),
        change_descriptor: Some(change_descriptor.clone()),
        network: Some(Network::Testnet),
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
        descriptor: None,
        change_descriptor: None,
        network: None,
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
pub fn persist_multiple_wallet_changesets<Store, CreateStores>(
    filename: &str,
    create_dbs: CreateStores,
) where
    CreateStores: Fn(&Path) -> anyhow::Result<(Store, Store)>,
    Store: WalletPersister,
    Store::Error: Debug,
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
    let change_descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[1].parse().unwrap();

    let changeset1 = ChangeSet {
        descriptor: Some(descriptor.clone()),
        change_descriptor: Some(change_descriptor.clone()),
        network: Some(Network::Testnet),
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
    let change_descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[3].parse().unwrap();

    let changeset2 = ChangeSet {
        descriptor: Some(descriptor.clone()),
        change_descriptor: Some(change_descriptor.clone()),
        network: Some(Network::Testnet),
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
/// We create a dummy [`ChangeSet`] with only network field populated, persist it and check if
/// loaded [`ChangeSet`] has the same [`Network`] as what we persisted.
pub fn persist_network<Store, CreateStore>(filename: &str, create_store: CreateStore)
where
    CreateStore: Fn(&Path) -> anyhow::Result<Store>,
    Store: WalletPersister,
    Store::Error: Debug,
{
    // create store
    let temp_dir = tempfile::tempdir().expect("must create tempdir");
    let file_path = temp_dir.path().join(filename);
    let mut store = create_store(&file_path).expect("store should get created");

    // initialize store
    let changeset = WalletPersister::initialize(&mut store)
        .expect("should initialize and load empty changeset");
    assert_eq!(changeset, ChangeSet::default());

    // persist the network
    let changeset = ChangeSet {
        network: Some(Network::Bitcoin),
        ..ChangeSet::default()
    };
    WalletPersister::persist(&mut store, &changeset).expect("should persist changeset");

    // read the persisted network
    let changeset_read =
        WalletPersister::initialize(&mut store).expect("should load persisted changeset");

    assert_eq!(changeset_read.network, Some(Network::Bitcoin));
}

/// tests if descriptors are being persisted correctly
///
/// [`ChangeSet`]: <https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.ChangeSet.html>
///
/// We create a dummy [`ChangeSet`] with only descriptor fields populated, persist it and check if
/// loaded [`ChangeSet`] has the same descriptors as what we persisted.
pub fn persist_keychains<Store, CreateStore>(filename: &str, create_store: CreateStore)
where
    CreateStore: Fn(&Path) -> anyhow::Result<Store>,
    Store: WalletPersister,
    Store::Error: Debug,
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
    let change_descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();

    let changeset = ChangeSet {
        descriptor: Some(descriptor.clone()),
        change_descriptor: Some(change_descriptor.clone()),
        ..ChangeSet::default()
    };

    WalletPersister::persist(&mut store, &changeset).expect("should persist descriptors");

    // load the descriptors
    let changeset_read =
        WalletPersister::initialize(&mut store).expect("should read persisted changeset");

    assert_eq!(changeset_read.descriptor.unwrap(), descriptor);
    assert_eq!(changeset_read.change_descriptor.unwrap(), change_descriptor);
}

/// tests if descriptor(in a single keychain wallet) is being persisted correctly
///
/// [`ChangeSet`]: <https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.ChangeSet.html>
///
/// We create a dummy [`ChangeSet`] with only descriptor field populated, persist it and check if
/// loaded [`ChangeSet`] has the same descriptor as what we persisted.
pub fn persist_single_keychain<Store, CreateStore>(filename: &str, create_store: CreateStore)
where
    CreateStore: Fn(&Path) -> anyhow::Result<Store>,
    Store: WalletPersister,
    Store::Error: Debug,
{
    // create store
    let temp_dir = tempfile::tempdir().expect("must create tempdir");
    let file_path = temp_dir.path().join(filename);
    let mut store = create_store(&file_path).expect("store should get created");

    // initialize store
    let changeset = WalletPersister::initialize(&mut store)
        .expect("should initialize and load empty changeset");
    assert_eq!(changeset, ChangeSet::default());

    // persist descriptor
    let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();

    let changeset = ChangeSet {
        descriptor: Some(descriptor.clone()),
        ..ChangeSet::default()
    };

    WalletPersister::persist(&mut store, &changeset).expect("should persist descriptors");

    // load the descriptor
    let changeset_read =
        WalletPersister::initialize(&mut store).expect("should read persisted changeset");

    assert_eq!(changeset_read.descriptor.unwrap(), descriptor);
    assert_eq!(changeset_read.change_descriptor, None);
}

/// Creates a [`ChangeSet`].
fn get_changeset(tx1: Transaction) -> ChangeSet {
    let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();
    let change_descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[1].parse().unwrap();

    let local_chain_changeset = local_chain::ChangeSet {
        blocks: [
            (910234, Some(hash!("B"))),
            (910233, Some(hash!("T"))),
            (910235, Some(hash!("C"))),
        ]
        .into(),
    };

    let txid1 = tx1.compute_txid();

    let conf_anchor: ConfirmationBlockTime = ConfirmationBlockTime {
        block_id: block_id!(910234, "B"),
        confirmation_time: 1755317160,
    };

    let outpoint = OutPoint::new(hash!("Rust"), 0);

    let tx_graph_changeset = tx_graph::ChangeSet::<ConfirmationBlockTime> {
        txs: [Arc::new(tx1)].into(),
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
        anchors: [(conf_anchor, txid1)].into(),
        last_seen: [(txid1, 1755317760)].into(),
        first_seen: [(txid1, 1755317750)].into(),
        last_evicted: [(txid1, 1755317760)].into(),
    };

    let keychain_txout_changeset = keychain_txout::ChangeSet {
        last_revealed: [
            (descriptor.descriptor_id(), 12),
            (change_descriptor.descriptor_id(), 10),
        ]
        .into(),
        spk_cache: [
            (
                descriptor.descriptor_id(),
                SpkIterator::new_with_range(&descriptor, 0..=37).collect(),
            ),
            (
                change_descriptor.descriptor_id(),
                SpkIterator::new_with_range(&change_descriptor, 0..=35).collect(),
            ),
        ]
        .into(),
    };

    let locked_outpoints_changeset = locked_outpoints::ChangeSet {
        outpoints: [(outpoint, true)].into(),
    };

    ChangeSet {
        descriptor: Some(descriptor.clone()),
        change_descriptor: Some(change_descriptor.clone()),
        network: Some(Network::Testnet),
        local_chain: local_chain_changeset,
        tx_graph: tx_graph_changeset,
        indexer: keychain_txout_changeset,
        locked_outpoints: locked_outpoints_changeset,
    }
}

/// Creates another [`ChangeSet`].
fn get_changeset_two(tx2: Transaction) -> ChangeSet {
    let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();

    let local_chain_changeset = local_chain::ChangeSet {
        blocks: [(910236, Some(hash!("BDK")))].into(),
    };

    let conf_anchor: ConfirmationBlockTime = ConfirmationBlockTime {
        block_id: block_id!(910236, "BDK"),
        confirmation_time: 1755317760,
    };

    let txid2 = tx2.compute_txid();

    let outpoint = OutPoint::new(hash!("Bitcoin_fixes_things"), 0);

    let tx_graph_changeset = tx_graph::ChangeSet::<ConfirmationBlockTime> {
        txs: [Arc::new(tx2)].into(),
        txouts: [(
            outpoint,
            TxOut {
                value: Amount::from_sat(10000),
                script_pubkey: spk_at_index(&descriptor, 21),
            },
        )]
        .into(),
        anchors: [(conf_anchor, txid2)].into(),
        last_seen: [(txid2, 1755317700)].into(),
        first_seen: [(txid2, 1755317700)].into(),
        last_evicted: [(txid2, 1755317760)].into(),
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

    ChangeSet {
        descriptor: None,
        change_descriptor: None,
        network: None,
        local_chain: local_chain_changeset,
        tx_graph: tx_graph_changeset,
        indexer: keychain_txout_changeset,
        locked_outpoints: locked_outpoints_changeset,
    }
}

/// Errors caused by a failed wallet persister test.
#[derive(Debug)]
pub enum PersistError {
    /// Change set mismatch
    ChangeSetMismatch {
        /// the resulting changeset
        got: Box<ChangeSet>,
        /// the expected changeset
        expected: Box<ChangeSet>,
    },
    /// The wallet persister implementation failed
    Persister(Box<dyn StdErr + 'static>),
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persister(e) => write!(f, "{e}"),
            Self::ChangeSetMismatch { got, expected } => {
                write!(f, "expected: {expected:?}, got: {got:?}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl StdErr for PersistError {}

impl PersistError {
    /// Converts `e` to a [`PersistError::Persister`].
    fn persister<E>(e: E) -> Self
    where
        E: StdErr + 'static,
    {
        Self::Persister(Box::new(e))
    }
}

/// Test the functionality of an [`AsyncWalletPersister`].
///
/// # Errors
///
/// If any of the following doesn't occur:
///
/// - The store must initially be empty
/// - The store must persist non-empty changesets
/// - The store must return the expected changeset after being persisted
pub async fn persist_wallet_changeset_async<F, P>(create_store: F) -> Result<(), PersistError>
where
    F: AsyncFn() -> Result<P, P::Error>,
    P: AsyncWalletPersister,
    P::Error: StdErr + 'static,
{
    use PersistError as E;

    // Create store
    let mut store = create_store().await.map_err(E::persister)?;
    let changeset = AsyncWalletPersister::initialize(&mut store)
        .await
        .map_err(E::persister)?;

    // A newly created store must return an empty changeset
    if !changeset.is_empty() {
        return Err(PersistError::ChangeSetMismatch {
            got: Box::new(changeset),
            expected: Box::new(ChangeSet::default()),
        });
    }

    let tx1 = create_one_inp_one_out_tx(hash!("We_are_all_Satoshi"), 30_000);
    let tx2 = create_one_inp_one_out_tx(tx1.compute_txid(), 20_000);

    // Persist changeset
    let mut expected_changeset = get_changeset(tx1);

    AsyncWalletPersister::persist(&mut store, &expected_changeset)
        .await
        .map_err(E::persister)?;

    let changeset_read = AsyncWalletPersister::initialize(&mut store)
        .await
        .map_err(E::persister)?;

    if changeset_read != expected_changeset {
        return Err(E::ChangeSetMismatch {
            got: Box::new(changeset_read),
            expected: Box::new(expected_changeset),
        });
    }

    // Persist another changeset
    let changeset_2 = get_changeset_two(tx2);

    AsyncWalletPersister::persist(&mut store, &changeset_2)
        .await
        .map_err(E::persister)?;

    let changeset_read = AsyncWalletPersister::initialize(&mut store)
        .await
        .map_err(E::persister)?;

    expected_changeset.merge(changeset_2);

    if changeset_read != expected_changeset {
        return Err(E::ChangeSetMismatch {
            got: Box::new(changeset_read),
            expected: Box::new(expected_changeset),
        });
    }

    Ok(())
}

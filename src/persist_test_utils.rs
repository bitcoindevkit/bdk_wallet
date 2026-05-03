//! Utilities for testing custom persistence backends for `bdk_wallet`

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;
use core::str::FromStr;

use bdk_chain::{
    keychain_txout, local_chain, tx_graph, ConfirmationBlockTime, DescriptorExt, Merge, SpkIterator,
};
use bitcoin::{
    absolute, secp256k1::Secp256k1, transaction, Address, Amount, Network, OutPoint, ScriptBuf,
    Transaction, TxIn, TxOut, Txid,
};
use miniscript::{Descriptor, DescriptorPublicKey};

use crate::{locked_outpoints, AsyncWalletPersister, ChangeSet, WalletPersister};

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

/// Tests if [`Wallet`](crate::Wallet) is being persisted correctly.
///
/// Persists a full [`ChangeSet`] and verifies it round-trips correctly. Then persists a second
/// [`ChangeSet`] and verifies the backend returns the merged result.
pub fn persist_wallet_changeset<F, P>(create_store: F) -> Result<(), PersistError>
where
    F: FnOnce() -> Result<P, P::Error>,
    P: WalletPersister,
    P::Error: core::error::Error + 'static,
{
    let mut persister = init_wallet_persister(create_store)?;
    let tx1 = create_one_inp_one_out_tx(hash!("We_are_all_Satoshi"), 30_000);
    let tx2 = create_one_inp_one_out_tx(tx1.compute_txid(), 20_000);
    let changeset1 = get_changeset(tx1);
    persist_changeset_verify(&mut persister, &changeset1, &changeset1)?;
    let changeset2 = get_changeset_two(tx2);
    let mut expected = changeset1;
    Merge::merge(&mut expected, changeset2.clone());
    persist_changeset_verify(&mut persister, &changeset2, &expected)
}

/// tests if multiple [`Wallet`](crate::Wallet)s can be persisted in a single file correctly
///
/// We create a dummy [`ChangeSet`] for first wallet and persist it then we create a dummy
/// [`ChangeSet`] for second wallet and persist that. Finally we load these two [`ChangeSet`]s and
/// check if they were persisted correctly.
pub fn persist_multiple_wallet_changesets<F, P>(create_stores: F) -> Result<(), PersistError>
where
    F: Fn() -> Result<(P, P), P::Error>,
    P: WalletPersister,
    P::Error: core::error::Error + 'static,
{
    use PersistError as E;

    // create stores
    let (mut store_first, mut store_sec) = create_stores().map_err(E::persister)?;

    // initialize first store
    let changeset = WalletPersister::initialize(&mut store_first).map_err(E::persister)?;

    if changeset != ChangeSet::default() {
        return Err(PersistError::ChangeSetMismatch {
            got: Box::new(changeset),
            expected: Box::new(ChangeSet::default()),
        });
    }

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
    WalletPersister::persist(&mut store_first, &changeset1).map_err(E::persister)?;

    // initialize second store
    let changeset = WalletPersister::initialize(&mut store_sec).map_err(E::persister)?;

    if changeset != ChangeSet::default() {
        return Err(PersistError::ChangeSetMismatch {
            got: Box::new(changeset),
            expected: Box::new(ChangeSet::default()),
        });
    }

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
    WalletPersister::persist(&mut store_sec, &changeset2).map_err(E::persister)?;

    // load first changeset
    let changeset_read = WalletPersister::initialize(&mut store_first).map_err(E::persister)?;

    if changeset_read != changeset1 {
        return Err(PersistError::ChangeSetMismatch {
            got: Box::new(changeset_read),
            expected: Box::new(changeset1),
        });
    }

    // load second changeset
    let changeset_read = WalletPersister::initialize(&mut store_sec).map_err(E::persister)?;

    if changeset_read != changeset2 {
        return Err(PersistError::ChangeSetMismatch {
            got: Box::new(changeset_read),
            expected: Box::new(changeset2),
        });
    }

    Ok(())
}

/// Tests if [`Network`] is being persisted correctly.
///
/// Persists a [`ChangeSet`] with only the network field set and verifies it round-trips correctly.
pub fn persist_network<F, P>(create_store: F) -> Result<(), PersistError>
where
    F: FnOnce() -> Result<P, P::Error>,
    P: WalletPersister,
    P::Error: core::error::Error + 'static,
{
    let mut persister = init_wallet_persister(create_store)?;
    let changeset = network_changeset();
    persist_changeset_verify(&mut persister, &changeset, &changeset)
}

/// Tests if descriptors are being persisted correctly.
///
/// First persists only the external descriptor (covering the single-keychain case), then persists
/// the change descriptor and verifies the backend returns both merged.
pub fn persist_keychains<F, P>(create_store: F) -> Result<(), PersistError>
where
    F: FnOnce() -> Result<P, P::Error>,
    P: WalletPersister,
    P::Error: core::error::Error + 'static,
{
    let mut persister = init_wallet_persister(create_store)?;
    // Round 1: single keychain (external descriptor only)
    let changeset1 = descriptor_changeset();
    persist_changeset_verify(&mut persister, &changeset1, &changeset1)?;
    // Round 2: add the change descriptor, verify both are returned
    let changeset2 = change_descriptor_changeset();
    let mut expected = changeset1;
    Merge::merge(&mut expected, changeset2.clone());
    persist_changeset_verify(&mut persister, &changeset2, &expected)
}

/// Initializes a new [`WalletPersister`] and checks that the persistence backend is empty.
///
/// # Errors
///
/// - If the persister's [`initialize`] function returns a non-empty [`ChangeSet`], then
///   [`PersistError::ChangeSetMismatch`] error occurs.
///
/// [`initialize`]: WalletPersister::initialize
fn init_wallet_persister<F, P>(create_store: F) -> Result<P, PersistError>
where
    F: FnOnce() -> Result<P, P::Error>,
    P: WalletPersister,
    P::Error: core::error::Error + 'static,
{
    let mut persister = create_store().map_err(PersistError::persister)?;
    let changeset = WalletPersister::initialize(&mut persister).map_err(PersistError::persister)?;
    if changeset != ChangeSet::default() {
        return Err(PersistError::ChangeSetMismatch {
            got: Box::new(changeset),
            expected: Box::new(ChangeSet::default()),
        });
    }
    Ok(persister)
}

/// Persists the `changeset`, and verifies the persister returns the `expected` upon
/// initializing the backend.
///
/// # Errors
///
/// - If the [`WalletPersister`] implementation fails
/// - If the newly initialized [`ChangeSet`] doesn't match `expected`
fn persist_changeset_verify<P>(
    persister: &mut P,
    changeset: &ChangeSet,
    expected: &ChangeSet,
) -> Result<(), PersistError>
where
    P: WalletPersister,
    P::Error: core::error::Error + 'static,
{
    WalletPersister::persist(persister, changeset).map_err(PersistError::persister)?;
    let changeset = WalletPersister::initialize(persister).map_err(PersistError::persister)?;
    if &changeset != expected {
        return Err(PersistError::ChangeSetMismatch {
            got: Box::new(changeset),
            expected: Box::new(expected.clone()),
        });
    }
    Ok(())
}

fn network_changeset() -> ChangeSet {
    ChangeSet {
        network: Some(Network::Bitcoin),
        ..Default::default()
    }
}

fn descriptor_changeset() -> ChangeSet {
    let descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[0].parse().unwrap();
    ChangeSet {
        descriptor: Some(descriptor),
        ..Default::default()
    }
}

fn change_descriptor_changeset() -> ChangeSet {
    let change_descriptor: Descriptor<DescriptorPublicKey> = DESCRIPTORS[1].parse().unwrap();
    ChangeSet {
        change_descriptor: Some(change_descriptor),
        ..Default::default()
    }
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

/// Creates a second [`ChangeSet`].
///
/// To correctly test a wallet persister this should return a different
/// [`ChangeSet`] than the one returned by [`get_changeset`].
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
    Persister(Box<dyn core::error::Error + 'static>),
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

impl core::error::Error for PersistError {}

impl PersistError {
    /// Converts `e` to a [`PersistError::Persister`].
    fn persister<E>(e: E) -> Self
    where
        E: core::error::Error + 'static,
    {
        Self::Persister(Box::new(e))
    }
}

/// Tests the functionality of an [`AsyncWalletPersister`].
///
/// # Errors
///
/// If any of the following occurs:
///
/// - A newly initialized [`AsyncWalletPersister`] isn't empty
/// - The [`AsyncWalletPersister`] fails to persist a wallet [`ChangeSet`]
/// - A mismatch of [`ChangeSet`] between what is read and persisted
pub async fn persist_wallet_changeset_async<F, P>(create_store: F) -> Result<(), PersistError>
where
    F: AsyncFnOnce() -> Result<P, P::Error>,
    P: AsyncWalletPersister,
    P::Error: core::error::Error + 'static,
{
    let mut persister = init_async_wallet_persister(create_store).await?;
    let tx1 = create_one_inp_one_out_tx(hash!("We_are_all_Satoshi"), 30_000);
    let tx2 = create_one_inp_one_out_tx(tx1.compute_txid(), 20_000);
    let changeset1 = get_changeset(tx1);
    persist_changeset_verify_async(&mut persister, &changeset1, &changeset1).await?;
    let changeset2 = get_changeset_two(tx2);
    let mut expected = changeset1;
    Merge::merge(&mut expected, changeset2.clone());
    persist_changeset_verify_async(&mut persister, &changeset2, &expected).await
}

/// Tests if descriptors are being persisted correctly by an [`AsyncWalletPersister`].
///
/// First persists only the external descriptor (covering the single-keychain case), then persists
/// the change descriptor and verifies the backend returns both merged.
pub async fn persist_keychains_async<F, P>(create_store: F) -> Result<(), PersistError>
where
    F: AsyncFnOnce() -> Result<P, P::Error>,
    P: AsyncWalletPersister,
    P::Error: core::error::Error + 'static,
{
    let mut persister = init_async_wallet_persister(create_store).await?;
    // Round 1: single keychain (external descriptor only)
    let changeset1 = descriptor_changeset();
    persist_changeset_verify_async(&mut persister, &changeset1, &changeset1).await?;
    // Round 2: add the change descriptor, verify both are returned
    let changeset2 = change_descriptor_changeset();
    let mut expected = changeset1;
    Merge::merge(&mut expected, changeset2.clone());
    persist_changeset_verify_async(&mut persister, &changeset2, &expected).await
}

/// Tests network persistence.
///
/// Persists a [`ChangeSet`] with only the network field set and verifies it round-trips correctly.
pub async fn persist_network_async<F, P>(create_store: F) -> Result<(), PersistError>
where
    F: AsyncFnOnce() -> Result<P, P::Error>,
    P: AsyncWalletPersister,
    P::Error: core::error::Error + 'static,
{
    let mut persister = init_async_wallet_persister(create_store).await?;
    let changeset = network_changeset();
    let expected = &changeset;
    persist_changeset_verify_async(&mut persister, &changeset, expected).await
}

/// Initializes a new [`AsyncWalletPersister`] and checks that the persistence backend is empty.
///
/// # Errors
///
/// - If the persister's [`initialize`] function returns a non-empty [`ChangeSet`], then
///   [`PersistError::ChangeSetMismatch`] error occurs.
///
/// [`initialize`]: AsyncWalletPersister::initialize
async fn init_async_wallet_persister<F, P>(create_store: F) -> Result<P, PersistError>
where
    F: AsyncFnOnce() -> Result<P, P::Error>,
    P: AsyncWalletPersister,
    P::Error: core::error::Error + 'static,
{
    let mut persister = create_store().await.map_err(PersistError::persister)?;
    let changeset = AsyncWalletPersister::initialize(&mut persister)
        .await
        .map_err(PersistError::persister)?;
    if changeset != ChangeSet::default() {
        return Err(PersistError::ChangeSetMismatch {
            got: Box::new(changeset),
            expected: Box::new(ChangeSet::default()),
        });
    }
    Ok(persister)
}

/// Persists the `changeset`, and verifies the persister returns the `expected` upon
/// initializing the backend.
///
/// # Errors
///
/// - If the [`AsyncWalletPersister`] implementation fails
/// - If the newly initialized [`ChangeSet`] doesn't match `expected`
async fn persist_changeset_verify_async<P>(
    persister: &mut P,
    changeset: &ChangeSet,
    expected: &ChangeSet,
) -> Result<(), PersistError>
where
    P: AsyncWalletPersister,
    P::Error: core::error::Error + 'static,
{
    AsyncWalletPersister::persist(persister, changeset)
        .await
        .map_err(PersistError::persister)?;
    let changeset = AsyncWalletPersister::initialize(persister)
        .await
        .map_err(PersistError::persister)?;
    if &changeset != expected {
        return Err(PersistError::ChangeSetMismatch {
            got: Box::new(changeset),
            expected: Box::new(expected.clone()),
        });
    }
    Ok(())
}

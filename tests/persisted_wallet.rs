use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Context;
use assert_matches::assert_matches;
use bdk_chain::DescriptorId;
use bdk_chain::{
    keychain_txout::DEFAULT_LOOKAHEAD, ChainPosition, ConfirmationBlockTime, DescriptorExt,
};
use bdk_wallet::coin_selection::InsufficientFunds;
use bdk_wallet::descriptor::IntoWalletDescriptor;
use bdk_wallet::error::CreateTxError;
use bdk_wallet::test_utils::*;
use bdk_wallet::{
    ChangeSet, KeychainKind, LoadError, LoadMismatch, LoadWithPersistError, Wallet, WalletPersister,
};
use bitcoin::constants::ChainHash;
use bitcoin::hashes::Hash;
use bitcoin::key::Secp256k1;
use bitcoin::{
    absolute, secp256k1, transaction, Amount, BlockHash, Network, NetworkKind, ScriptBuf,
    Transaction, TxOut,
};
use miniscript::{Descriptor, DescriptorPublicKey};

use bdk_wallet::persist_test_utils::{
    persist_keychains, persist_network, persist_single_keychain, persist_wallet_changeset,
};

mod common;
use common::*;

const DB_MAGIC: &[u8] = &[0x21, 0x24, 0x48];

#[test]
fn wallet_is_persisted() -> anyhow::Result<()> {
    type SpkCacheChangeSet = BTreeMap<DescriptorId, BTreeMap<u32, ScriptBuf>>;

    /// Check whether the spk-cache field of the changeset contains the expected spk indices.
    fn check_cache_cs(
        cache_cs: &SpkCacheChangeSet,
        expected: impl IntoIterator<Item = (KeychainKind, impl IntoIterator<Item = u32>)>,
        msg: impl AsRef<str>,
    ) {
        let secp = secp256k1::Secp256k1::new();
        let (external, internal) = get_test_tr_single_sig_xprv_and_change_desc();
        let (external_desc, _) = external
            .into_wallet_descriptor(&secp, NetworkKind::Test)
            .unwrap();
        let (internal_desc, _) = internal
            .into_wallet_descriptor(&secp, NetworkKind::Test)
            .unwrap();
        let external_did = external_desc.descriptor_id();
        let internal_did = internal_desc.descriptor_id();

        let cache_cmp = cache_cs
            .iter()
            .map(|(did, spks)| {
                let kind: KeychainKind;
                if did == &external_did {
                    kind = KeychainKind::External;
                } else if did == &internal_did {
                    kind = KeychainKind::Internal;
                } else {
                    unreachable!();
                }
                let spk_indices = spks.keys().copied().collect::<BTreeSet<u32>>();
                (kind, spk_indices)
            })
            .filter(|(_, spk_indices)| !spk_indices.is_empty())
            .collect::<BTreeMap<KeychainKind, BTreeSet<u32>>>();

        let expected_cmp = expected
            .into_iter()
            .map(|(kind, indices)| (kind, indices.into_iter().collect::<BTreeSet<u32>>()))
            .filter(|(_, spk_indices)| !spk_indices.is_empty())
            .collect::<BTreeMap<KeychainKind, BTreeSet<u32>>>();

        assert_eq!(cache_cmp, expected_cmp, "{}", msg.as_ref());
    }

    fn staged_cache(wallet: &Wallet) -> SpkCacheChangeSet {
        wallet.staged().map_or(SpkCacheChangeSet::default(), |cs| {
            cs.indexer.spk_cache.clone()
        })
    }

    fn run<Db, CreateDb, OpenDb>(
        filename: &str,
        create_db: CreateDb,
        open_db: OpenDb,
    ) -> anyhow::Result<()>
    where
        CreateDb: Fn(&Path) -> anyhow::Result<Db>,
        OpenDb: Fn(&Path) -> anyhow::Result<Db>,
        Db: WalletPersister,
        Db::Error: std::error::Error + Send + Sync + 'static,
    {
        let temp_dir = tempfile::tempdir().expect("must create tempdir");
        let file_path = temp_dir.path().join(filename);
        let (external_desc, internal_desc) = get_test_tr_single_sig_xprv_and_change_desc();

        // create new wallet
        let wallet_spk_index = {
            let mut db = create_db(&file_path)?;
            let mut wallet = Wallet::create(external_desc, internal_desc)
                .network(Network::Testnet)
                .use_spk_cache(true)
                .create_wallet(&mut db)?;

            wallet.reveal_next_address(KeychainKind::External);

            check_cache_cs(
                &staged_cache(&wallet),
                [
                    (KeychainKind::External, 0..DEFAULT_LOOKAHEAD + 1),
                    (KeychainKind::Internal, 0..DEFAULT_LOOKAHEAD),
                ],
                "cache cs must return initial set + the external index that was just derived",
            );

            // persist new wallet changes
            assert!(wallet.persist(&mut db)?, "must write");
            wallet.spk_index().clone()
        };

        // recover wallet
        {
            let mut db = open_db(&file_path).context("failed to recover db")?;
            let wallet = Wallet::load()
                .descriptor(KeychainKind::External, Some(external_desc))
                .descriptor(KeychainKind::Internal, Some(internal_desc))
                .check_network(Network::Testnet)
                .load_wallet(&mut db)?
                .expect("wallet must exist");

            assert_eq!(wallet.network(), Network::Testnet);
            assert_eq!(
                wallet.spk_index().keychains().collect::<Vec<_>>(),
                wallet_spk_index.keychains().collect::<Vec<_>>()
            );
            assert_eq!(
                wallet.spk_index().last_revealed_indices(),
                wallet_spk_index.last_revealed_indices()
            );
            let secp = Secp256k1::new();
            assert_eq!(
                *wallet.public_descriptor(KeychainKind::External),
                external_desc
                    .into_wallet_descriptor(&secp, wallet.network().into())
                    .unwrap()
                    .0
            );
        }

        // Test SPK cache
        {
            let mut db = open_db(&file_path).context("failed to recover db")?;
            let mut wallet = Wallet::load()
                .check_network(Network::Testnet)
                .use_spk_cache(true)
                .load_wallet(&mut db)?
                .expect("wallet must exist");

            assert!(wallet.staged().is_none());

            let revealed_external_addr = wallet.reveal_next_address(KeychainKind::External);
            check_cache_cs(
                &staged_cache(&wallet),
                [(
                    KeychainKind::External,
                    [revealed_external_addr.index + DEFAULT_LOOKAHEAD],
                )],
                "must only persist the revealed+LOOKAHEAD indexed external spk",
            );

            // Clear the stage
            let _ = wallet.take_staged();

            let revealed_internal_addr = wallet.reveal_next_address(KeychainKind::Internal);
            check_cache_cs(
                &staged_cache(&wallet),
                [(
                    KeychainKind::Internal,
                    [revealed_internal_addr.index + DEFAULT_LOOKAHEAD],
                )],
                "must only persist the revealed+LOOKAHEAD indexed internal spk",
            );
        }

        // SPK cache requires load params
        {
            let mut db = open_db(&file_path).context("failed to recover db")?;
            let mut wallet = Wallet::load()
                .check_network(Network::Testnet)
                // .use_spk_cache(false)
                .load_wallet(&mut db)?
                .expect("wallet must exist");

            let internal_did = wallet
                .public_descriptor(KeychainKind::Internal)
                .descriptor_id();

            assert!(wallet.staged().is_none());

            let _addr = wallet.reveal_next_address(KeychainKind::Internal);
            let cs = wallet.staged().expect("we should have staged a changeset");
            assert_eq!(cs.indexer.last_revealed.get(&internal_did), Some(&0));
            assert!(
                cs.indexer.spk_cache.is_empty(),
                "we didn't set `use_spk_cache`"
            );
        }

        Ok(())
    }

    run(
        "store.db",
        |path| Ok(bdk_file_store::Store::create(DB_MAGIC, path)?),
        |path| Ok(bdk_file_store::Store::load(DB_MAGIC, path)?.0),
    )?;
    run::<bdk_chain::rusqlite::Connection, _, _>(
        "store.sqlite",
        |path| Ok(bdk_chain::rusqlite::Connection::open(path)?),
        |path| Ok(bdk_chain::rusqlite::Connection::open(path)?),
    )?;

    Ok(())
}

#[test]
fn wallet_load_checks() -> anyhow::Result<()> {
    fn run<Db, CreateDb, OpenDb>(
        filename: &str,
        create_db: CreateDb,
        open_db: OpenDb,
    ) -> anyhow::Result<()>
    where
        CreateDb: Fn(&Path) -> anyhow::Result<Db>,
        OpenDb: Fn(&Path) -> anyhow::Result<Db>,
        Db: WalletPersister + std::fmt::Debug,
        Db::Error: std::error::Error + Send + Sync + 'static,
    {
        let temp_dir = tempfile::tempdir().expect("must create tempdir");
        let file_path = temp_dir.path().join(filename);
        let network = Network::Testnet;
        let (external_desc, internal_desc) = get_test_tr_single_sig_xprv_and_change_desc();

        // create new wallet
        let _ = Wallet::create(external_desc, internal_desc)
            .network(network)
            .create_wallet(&mut create_db(&file_path)?)?;

        assert_matches!(
            Wallet::load()
                .check_network(Network::Regtest)
                .load_wallet(&mut open_db(&file_path)?),
            Err(LoadWithPersistError::InvalidChangeSet(LoadError::Mismatch(
                LoadMismatch::Network {
                    loaded: Network::Testnet,
                    expected: Network::Regtest,
                }
            ))),
            "unexpected network check result: Regtest (check) is not Testnet (loaded)",
        );
        let mainnet_hash = BlockHash::from_byte_array(ChainHash::BITCOIN.to_bytes());
        assert_matches!(
            Wallet::load().check_genesis_hash(mainnet_hash).load_wallet(&mut open_db(&file_path)?),
            Err(LoadWithPersistError::InvalidChangeSet(LoadError::Mismatch(LoadMismatch::Genesis { .. }))),
            "unexpected genesis hash check result: mainnet hash (check) is not testnet hash (loaded)",
        );
        assert_matches!(
            Wallet::load()
                .descriptor(KeychainKind::External, Some(internal_desc))
                .load_wallet(&mut open_db(&file_path)?),
            Err(LoadWithPersistError::InvalidChangeSet(LoadError::Mismatch(
                LoadMismatch::Descriptor { .. }
            ))),
            "unexpected descriptors check result",
        );
        assert_matches!(
            Wallet::load()
                .descriptor(KeychainKind::External, Option::<&str>::None)
                .load_wallet(&mut open_db(&file_path)?),
            Err(LoadWithPersistError::InvalidChangeSet(LoadError::Mismatch(
                LoadMismatch::Descriptor { .. }
            ))),
            "unexpected descriptors check result",
        );
        // check setting keymaps
        let (_, external_keymap) = parse_descriptor(external_desc);
        let (_, internal_keymap) = parse_descriptor(internal_desc);
        let wallet = Wallet::load()
            .keymap(KeychainKind::External, external_keymap)
            .keymap(KeychainKind::Internal, internal_keymap)
            .load_wallet(&mut open_db(&file_path)?)
            .expect("db should not fail")
            .expect("wallet was persisted");
        for keychain in [KeychainKind::External, KeychainKind::Internal] {
            let keymap = wallet.get_signers(keychain).as_key_map(wallet.secp_ctx());
            assert!(
                !keymap.is_empty(),
                "load should populate keymap for keychain {keychain:?}"
            );
        }
        Ok(())
    }

    run(
        "store.db",
        |path| Ok(bdk_file_store::Store::<ChangeSet>::create(DB_MAGIC, path)?),
        |path| Ok(bdk_file_store::Store::<ChangeSet>::load(DB_MAGIC, path)?.0),
    )?;
    run(
        "store.sqlite",
        |path| Ok(bdk_chain::rusqlite::Connection::open(path)?),
        |path| Ok(bdk_chain::rusqlite::Connection::open(path)?),
    )?;

    Ok(())
}

#[test]
fn wallet_should_persist_anchors_and_recover() {
    use bdk_chain::rusqlite;
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("wallet.db");
    let mut db = rusqlite::Connection::open(db_path).unwrap();

    let desc = get_test_tr_single_sig_xprv();
    let mut wallet = Wallet::create_single(desc)
        .network(Network::Testnet)
        .create_wallet(&mut db)
        .unwrap();
    let small_output_tx = Transaction {
        input: vec![],
        output: vec![TxOut {
            script_pubkey: wallet
                .next_unused_address(KeychainKind::External)
                .script_pubkey(),
            value: Amount::from_sat(25_000),
        }],
        version: transaction::Version::non_standard(0),
        lock_time: absolute::LockTime::ZERO,
    };
    let txid = small_output_tx.compute_txid();
    insert_tx(&mut wallet, small_output_tx);
    let expected_anchor = ConfirmationBlockTime {
        block_id: wallet.latest_checkpoint().block_id(),
        confirmation_time: 200,
    };
    insert_anchor(&mut wallet, txid, expected_anchor);
    assert!(wallet.persist(&mut db).unwrap());

    // should recover persisted wallet
    let secp = wallet.secp_ctx();
    let (_, keymap) = <Descriptor<DescriptorPublicKey>>::parse_descriptor(secp, desc).unwrap();
    assert!(!keymap.is_empty());
    let wallet = Wallet::load()
        .descriptor(KeychainKind::External, Some(desc))
        .extract_keys()
        .load_wallet(&mut db)
        .unwrap()
        .expect("must have loaded changeset");
    // stored anchor should be retrieved in the same condition it was persisted
    if let ChainPosition::Confirmed {
        anchor: obtained_anchor,
        ..
    } = wallet
        .get_tx(txid)
        .expect("should retrieve stored tx")
        .chain_position
    {
        assert_eq!(obtained_anchor, expected_anchor)
    } else {
        panic!("Should have got ChainPosition::Confirmed)");
    }
}

#[test]
fn single_descriptor_wallet_persist_and_recover() {
    use bdk_chain::miniscript::Descriptor;
    use bdk_chain::miniscript::DescriptorPublicKey;
    use bdk_chain::rusqlite;
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("wallet.db");
    let mut db = rusqlite::Connection::open(db_path).unwrap();

    let desc = get_test_tr_single_sig_xprv();
    let mut wallet = Wallet::create_single(desc)
        .network(Network::Testnet)
        .create_wallet(&mut db)
        .unwrap();
    let _ = wallet.reveal_addresses_to(KeychainKind::External, 2);
    assert!(wallet.persist(&mut db).unwrap());

    // should recover persisted wallet
    let secp = wallet.secp_ctx();
    let (_, keymap) = <Descriptor<DescriptorPublicKey>>::parse_descriptor(secp, desc).unwrap();
    assert!(!keymap.is_empty());
    let wallet = Wallet::load()
        .descriptor(KeychainKind::External, Some(desc))
        .extract_keys()
        .load_wallet(&mut db)
        .unwrap()
        .expect("must have loaded changeset");
    assert_eq!(wallet.derivation_index(KeychainKind::External), Some(2));
    // should have private key
    assert_eq!(
        wallet.get_signers(KeychainKind::External).as_key_map(secp),
        keymap,
    );

    // should error on wrong internal params
    let desc = get_test_wpkh();
    let (exp_desc, _) = <Descriptor<DescriptorPublicKey>>::parse_descriptor(secp, desc).unwrap();
    let err = Wallet::load()
        .descriptor(KeychainKind::Internal, Some(desc))
        .extract_keys()
        .load_wallet(&mut db);
    assert_matches!(
        err,
        Err(LoadWithPersistError::InvalidChangeSet(LoadError::Mismatch(LoadMismatch::Descriptor { keychain, loaded, expected })))
        if keychain == KeychainKind::Internal && loaded.is_none() && expected == Some(Box::new(exp_desc)),
        "single descriptor wallet should refuse change descriptor param"
    );
}

#[test]
fn wallet_changeset_is_persisted() {
    let tmpdir = tempfile::tempdir().unwrap();

    // Test file_store
    persist_wallet_changeset(|| {
        bdk_file_store::Store::load_or_create(DB_MAGIC, tmpdir.path().join("store.db"))
            .map(|(store, _)| store)
            .map_err(bdk_wallet::FileStoreError::Load)
    })
    .expect("failed to persist wallet changeset");

    // Test rusqlite Connection
    persist_wallet_changeset(|| {
        bdk_chain::rusqlite::Connection::open(tmpdir.path().join("store.sqlite"))
    })
    .expect("failed to persist wallet changeset");
}

#[test]
fn keychains_are_persisted() {
    let tmpdir = tempfile::tempdir().unwrap();

    // Test file_store
    persist_keychains(|| {
        bdk_file_store::Store::load_or_create(DB_MAGIC, tmpdir.path().join("store.db"))
            .map(|(store, _)| store)
            .map_err(bdk_wallet::FileStoreError::Load)
    })
    .expect("failed to persist keychains");

    // Test rusqlite Connection
    persist_keychains(|| bdk_chain::rusqlite::Connection::open(tmpdir.path().join("store.sqlite")))
        .expect("failed to persist keychains");
}

#[test]
fn single_keychain_is_persisted() {
    let tmpdir = tempfile::tempdir().unwrap();

    // Test file_store
    persist_single_keychain(|| {
        bdk_file_store::Store::load_or_create(DB_MAGIC, tmpdir.path().join("store.db"))
            .map(|(store, _)| store)
            .map_err(bdk_wallet::FileStoreError::Load)
    })
    .expect("failed to persist single keychain");

    // Test rusqlite Connection
    persist_single_keychain(|| {
        bdk_chain::rusqlite::Connection::open(tmpdir.path().join("store.sqlite"))
    })
    .expect("failed to persist single keychain");
}

#[test]
fn network_is_persisted() {
    let tmpdir = tempfile::tempdir().unwrap();

    // Test file_store
    persist_network(|| {
        bdk_file_store::Store::load_or_create(DB_MAGIC, tmpdir.path().join("store.db"))
            .map(|(store, _)| store)
            .map_err(bdk_wallet::FileStoreError::Load)
    })
    .expect("failed to persist network");

    // Test rusqlite Connection
    persist_network(|| bdk_chain::rusqlite::Connection::open(tmpdir.path().join("store.sqlite")))
        .expect("failed to persist network");
}

#[test]
fn test_lock_outpoint_persist() -> anyhow::Result<()> {
    use bdk_chain::rusqlite;
    let mut conn = rusqlite::Connection::open_in_memory()?;

    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    // Receive coins.
    let mut outpoints = vec![];
    for i in 0..3 {
        let op = receive_output(&mut wallet, Amount::from_sat(10_000), ReceiveTo::Mempool(i));
        outpoints.push(op);
    }

    // Test: lock outpoints
    let unspent = wallet.list_unspent().collect::<Vec<_>>();
    assert!(!unspent.is_empty());
    for utxo in unspent {
        wallet.lock_outpoint(utxo.outpoint);
        assert!(
            wallet.is_outpoint_locked(utxo.outpoint),
            "Expect outpoint is locked"
        );
    }
    wallet.persist(&mut conn)?;

    // Test: The lock value is persistent
    {
        wallet = Wallet::load()
            .load_wallet(&mut conn)?
            .expect("wallet is persisted");

        let unspent = wallet.list_unspent().collect::<Vec<_>>();
        assert!(!unspent.is_empty());
        for utxo in unspent {
            assert!(
                wallet.is_outpoint_locked(utxo.outpoint),
                "Expect recover lock value"
            );
        }
        let locked_unspent = wallet.list_locked_unspent().collect::<Vec<_>>();
        assert_eq!(locked_unspent, outpoints);

        // Test: Locked outpoints are excluded from coin selection
        let addr = wallet.next_unused_address(KeychainKind::External).address;
        let mut tx_builder = wallet.build_tx();
        tx_builder.add_recipient(addr, Amount::from_sat(10_000));
        let res = tx_builder.finish();
        assert!(
            matches!(
                res,
                Err(CreateTxError::CoinSelection(InsufficientFunds {
                    available: Amount::ZERO,
                    ..
                })),
            ),
            "Locked outpoints should not be selected",
        );
    }

    // Test: Unlock outpoints
    {
        wallet = Wallet::load()
            .load_wallet(&mut conn)?
            .expect("wallet is persisted");

        let unspent = wallet.list_unspent().collect::<Vec<_>>();
        for utxo in &unspent {
            wallet.unlock_outpoint(utxo.outpoint);
            assert!(
                !wallet.is_outpoint_locked(utxo.outpoint),
                "Expect outpoint is not locked"
            );
        }
        assert!(wallet.list_locked_outpoints().next().is_none());
        assert!(wallet.list_locked_unspent().next().is_none());
    }

    Ok(())
}

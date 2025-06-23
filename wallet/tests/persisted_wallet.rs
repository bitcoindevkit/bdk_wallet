use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use assert_matches::assert_matches;
use bdk_chain::{
    keychain_txout::DEFAULT_LOOKAHEAD, ChainPosition, ConfirmationBlockTime, DescriptorExt,
};
use bdk_wallet::descriptor::IntoWalletDescriptor;
use bdk_wallet::test_utils::*;
use bdk_wallet::{
    ChangeSet, KeychainKind, LoadError, LoadMismatch, LoadWithPersistError, Wallet, WalletPersister,
};
use bitcoin::constants::ChainHash;
use bitcoin::hashes::Hash;
use bitcoin::key::Secp256k1;
use bitcoin::{absolute, transaction, Amount, BlockHash, Network, ScriptBuf, Transaction, TxOut};
use miniscript::{Descriptor, DescriptorPublicKey};

mod common;
use common::*;

const DB_MAGIC: &[u8] = &[0x21, 0x24, 0x48];

#[test]
fn wallet_is_persisted() -> anyhow::Result<()> {
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
                    .into_wallet_descriptor(&secp, wallet.network())
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

            let external_did = wallet
                .public_descriptor(KeychainKind::External)
                .descriptor_id();
            let internal_did = wallet
                .public_descriptor(KeychainKind::Internal)
                .descriptor_id();

            assert!(wallet.staged().is_none());

            let _addr = wallet.reveal_next_address(KeychainKind::External);
            let cs = wallet.staged().expect("we should have staged a changeset");
            assert!(!cs.indexer.spk_cache.is_empty(), "failed to cache spks");
            assert_eq!(cs.indexer.spk_cache.len(), 2, "we persisted two keychains");
            let spk_cache: &BTreeMap<u32, ScriptBuf> =
                cs.indexer.spk_cache.get(&external_did).unwrap();
            assert_eq!(spk_cache.len() as u32, 1 + 1 + DEFAULT_LOOKAHEAD);
            assert_eq!(spk_cache.keys().last(), Some(&26));
            let spk_cache = cs.indexer.spk_cache.get(&internal_did).unwrap();
            assert_eq!(spk_cache.len() as u32, DEFAULT_LOOKAHEAD);
            assert_eq!(spk_cache.keys().last(), Some(&24));
            // Clear the stage
            let _ = wallet.take_staged();
            let _addr = wallet.reveal_next_address(KeychainKind::Internal);
            let cs = wallet.staged().unwrap();
            assert_eq!(cs.indexer.spk_cache.len(), 1);
            let spk_cache = cs.indexer.spk_cache.get(&internal_did).unwrap();
            assert_eq!(spk_cache.len(), 1);
            assert_eq!(spk_cache.keys().next(), Some(&25));
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
        if keychain == KeychainKind::Internal && loaded.is_none() && expected == Some(exp_desc),
        "single descriptor wallet should refuse change descriptor param"
    );
}

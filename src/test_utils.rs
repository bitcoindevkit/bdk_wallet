//! `bdk_wallet` test utilities

use alloc::string::ToString;
use alloc::sync::Arc;
use core::str::FromStr;
use miniscript::descriptor::{DescriptorSecretKey, KeyMap};
use std::collections::BTreeMap;

use bdk_chain::{BlockId, ConfirmationBlockTime, TxUpdate};
use bitcoin::{
    absolute,
    hashes::Hash,
    key::Secp256k1,
    psbt::{GetKey, GetKeyError, KeyRequest},
    transaction, Address, Amount, BlockHash, FeeRate, Network, OutPoint, Transaction, TxIn, TxOut,
    Txid,
};

use crate::{descriptor::check_wallet_descriptor, KeychainKind, Update, Wallet};

#[derive(Debug, Clone)]
/// A wrapper over the [`KeyMap`] type that has the `GetKey` trait implementation for signing.
pub struct SignerWrapper {
    key_map: KeyMap,
}

impl SignerWrapper {
    /// Creates a new [`SignerWrapper`] for the given [`KeyMap`].
    pub fn new(key_map: KeyMap) -> Self {
        Self { key_map }
    }
}

impl GetKey for SignerWrapper {
    type Error = GetKeyError;

    fn get_key<C: bitcoin::secp256k1::Signing>(
        &self,
        key_request: KeyRequest,
        secp: &bitcoin::key::Secp256k1<C>,
    ) -> Result<Option<bitcoin::PrivateKey>, Self::Error> {
        for key_map in self.key_map.iter() {
            let (_, desc_sk) = key_map;
            let wrapper = DescriptorSecretKeyWrapper::new(desc_sk.clone());
            match wrapper.get_key(key_request.clone(), secp) {
                Ok(Some(private_key)) => return Ok(Some(private_key)),
                Ok(None) => continue,
                // TODO: (@leonardo) how should we handle this ?
                // we can't error-out on this, because the valid signing key can be in the next
                // iterations.
                Err(_) => continue,
            }
        }
        Ok(None)
    }
}

/// Wrapper for [`DescriptorSecretKey`] that implements the [`GetKey`] trait for signing.
pub struct DescriptorSecretKeyWrapper(DescriptorSecretKey);

impl DescriptorSecretKeyWrapper {
    /// Creates a new [`DescriptorSecretKeyWrapper`] from a [`DescriptorSecretKey`].
    pub fn new(desc_sk: DescriptorSecretKey) -> Self {
        Self(desc_sk)
    }
}

impl GetKey for DescriptorSecretKeyWrapper {
    type Error = GetKeyError;

    fn get_key<C: bitcoin::secp256k1::Signing>(
        &self,
        key_request: KeyRequest,
        secp: &Secp256k1<C>,
    ) -> Result<Option<bitcoin::PrivateKey>, Self::Error> {
        match (&self.0, key_request) {
            (DescriptorSecretKey::Single(single_priv), key_request) => {
                let private_key = single_priv.key;
                let public_key = private_key.public_key(secp);
                let pubkey_map = BTreeMap::from([(public_key, private_key)]);
                return pubkey_map.get_key(key_request, secp);
            }
            (DescriptorSecretKey::XPrv(descriptor_xkey), KeyRequest::Pubkey(public_key)) => {
                let private_key = descriptor_xkey.xkey.private_key;
                let pk = private_key.public_key(secp);
                if public_key.inner.eq(&pk) {
                    return Ok(Some(
                        descriptor_xkey
                            .xkey
                            .derive_priv(secp, &descriptor_xkey.derivation_path)
                            .map_err(GetKeyError::Bip32)?
                            .to_priv(),
                    ));
                }
            }
            (
                DescriptorSecretKey::XPrv(descriptor_xkey),
                ref key_request @ KeyRequest::Bip32(ref key_source),
            ) => {
                if let Some(key) = descriptor_xkey.xkey.get_key(key_request.clone(), secp)? {
                    return Ok(Some(key));
                }

                if let Some(_derivation_path) = descriptor_xkey.matches(key_source, secp) {
                    let (_fp, derivation_path) = key_source;

                    if let Some((_fp, origin_derivation_path)) = &descriptor_xkey.origin {
                        let derivation_path = &derivation_path[origin_derivation_path.len()..];
                        return Ok(Some(
                            descriptor_xkey
                                .xkey
                                .derive_priv(secp, &derivation_path)
                                .map_err(GetKeyError::Bip32)?
                                .to_priv(),
                        ));
                    } else {
                        return Ok(Some(
                            descriptor_xkey
                                .xkey
                                .derive_priv(secp, derivation_path)
                                .map_err(GetKeyError::Bip32)?
                                .to_priv(),
                        ));
                    };
                }
            }
            (DescriptorSecretKey::XPrv(_), KeyRequest::XOnlyPubkey(_)) => {
                return Err(GetKeyError::NotSupported)
            }
            (DescriptorSecretKey::MultiXPrv(_), _) => unimplemented!(),
            _ => unreachable!(),
        }
        Ok(None)
    }
}

/// Create the [`CreateParams`] for the provided testing `descriptor` and `change_descriptor`.
pub fn get_wallet_params(descriptor: &str, change_descriptor: Option<&str>) -> crate::CreateParams {
    if let Some(change_desc) = change_descriptor {
        Wallet::create(descriptor.to_string(), change_desc.to_string())
    } else {
        Wallet::create_single(descriptor.to_string())
    }
}

/// Create a new [`SignerWrapper`] for the provided testing `descriptor` and `change_descriptor`.
pub fn get_wallet_signer(descriptor: &str, change_descriptor: Option<&str>) -> SignerWrapper {
    let secp = Secp256k1::new();
    let params = get_wallet_params(descriptor, change_descriptor).network(Network::Regtest);

    let network = params.network;

    let (descriptor, mut descriptor_keymap) = (params.descriptor)(&secp, network).unwrap();
    check_wallet_descriptor(&descriptor).unwrap();
    descriptor_keymap.extend(params.descriptor_keymap);

    if let Some(change_descriptor) = params.change_descriptor {
        let (change_descriptor, mut change_keymap) = change_descriptor(&secp, network).unwrap();
        check_wallet_descriptor(&change_descriptor).unwrap();
        change_keymap.extend(params.change_descriptor_keymap);
        descriptor_keymap.extend(change_keymap)
    }

    SignerWrapper::new(descriptor_keymap)
}

/// Create a new [`SignerWrapper`] for the provided testing `descriptor`.
pub fn get_wallet_signer_single(descriptor: &str) -> SignerWrapper {
    get_wallet_signer(descriptor, None)
}

/// Return a fake wallet that appears to be funded for testing.
///
/// The funded wallet contains a tx with a 76_000 sats input and two outputs, one spending 25_000
/// to a foreign address and one returning 50_000 back to the wallet. The remaining 1000
/// sats are the transaction fee.
pub fn get_funded_wallet(descriptor: &str, change_descriptor: &str) -> (Wallet, Txid) {
    new_funded_wallet(descriptor, Some(change_descriptor))
}

fn new_funded_wallet(descriptor: &str, change_descriptor: Option<&str>) -> (Wallet, Txid) {
    let params = if let Some(change_desc) = change_descriptor {
        Wallet::create(descriptor.to_string(), change_desc.to_string())
    } else {
        Wallet::create_single(descriptor.to_string())
    };

    let mut wallet = params
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("descriptors must be valid");

    let receive_address = wallet.peek_address(KeychainKind::External, 0).address;
    let sendto_address = Address::from_str("bcrt1q3qtze4ys45tgdvguj66zrk4fu6hq3a3v9pfly5")
        .expect("address")
        .require_network(Network::Regtest)
        .unwrap();

    let tx0 = Transaction {
        output: vec![TxOut {
            value: Amount::from_sat(76_000),
            script_pubkey: receive_address.script_pubkey(),
        }],
        ..new_tx(0)
    };

    let tx1 = Transaction {
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: tx0.compute_txid(),
                vout: 0,
            },
            ..Default::default()
        }],
        output: vec![
            TxOut {
                value: Amount::from_sat(50_000),
                script_pubkey: receive_address.script_pubkey(),
            },
            TxOut {
                value: Amount::from_sat(25_000),
                script_pubkey: sendto_address.script_pubkey(),
            },
        ],
        ..new_tx(0)
    };

    insert_checkpoint(
        &mut wallet,
        BlockId {
            height: 42,
            hash: BlockHash::all_zeros(),
        },
    );
    insert_checkpoint(
        &mut wallet,
        BlockId {
            height: 1_000,
            hash: BlockHash::all_zeros(),
        },
    );
    insert_checkpoint(
        &mut wallet,
        BlockId {
            height: 2_000,
            hash: BlockHash::all_zeros(),
        },
    );

    insert_tx(&mut wallet, tx0.clone());
    insert_anchor(
        &mut wallet,
        tx0.compute_txid(),
        ConfirmationBlockTime {
            block_id: BlockId {
                height: 1_000,
                hash: BlockHash::all_zeros(),
            },
            confirmation_time: 100,
        },
    );

    insert_tx(&mut wallet, tx1.clone());
    insert_anchor(
        &mut wallet,
        tx1.compute_txid(),
        ConfirmationBlockTime {
            block_id: BlockId {
                height: 2_000,
                hash: BlockHash::all_zeros(),
            },
            confirmation_time: 200,
        },
    );

    (wallet, tx1.compute_txid())
}

/// Return a fake wallet that appears to be funded for testing.
///
/// The funded wallet contains a tx with a 76_000 sats input and two outputs, one spending 25_000
/// to a foreign address and one returning 50_000 back to the wallet. The remaining 1000
/// sats are the transaction fee.
pub fn get_funded_wallet_single(descriptor: &str) -> (Wallet, Txid) {
    new_funded_wallet(descriptor, None)
}

/// Get funded segwit wallet
pub fn get_funded_wallet_wpkh() -> (Wallet, Txid) {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();
    get_funded_wallet(desc, change_desc)
}

/// `pkh` single key descriptor
pub fn get_test_pkh() -> &'static str {
    "pkh(cNJFgo1driFnPcBdBX8BrJrpxchBWXwXCvNH5SoSkdcF6JXXwHMm)"
}

/// `wpkh` single key descriptor
pub fn get_test_wpkh() -> &'static str {
    "wpkh(cVpPVruEDdmutPzisEsYvtST1usBR3ntr8pXSyt6D2YYqXRyPcFW)"
}

/// `wpkh` xpriv and change descriptor
pub fn get_test_wpkh_and_change_desc() -> (&'static str, &'static str) {
    ("wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)",
    "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)")
}

/// `wsh` descriptor with policy `and(pk(A),older(6))`
pub fn get_test_single_sig_csv() -> &'static str {
    "wsh(and_v(v:pk(cVpPVruEDdmutPzisEsYvtST1usBR3ntr8pXSyt6D2YYqXRyPcFW),older(6)))"
}

/// `wsh` descriptor with policy `or(pk(A),and(pk(B),older(144)))`
pub fn get_test_a_or_b_plus_csv() -> &'static str {
    "wsh(or_d(pk(cRjo6jqfVNP33HhSS76UhXETZsGTZYx8FMFvR9kpbtCSV1PmdZdu),and_v(v:pk(cMnkdebixpXMPfkcNEjjGin7s94hiehAH4mLbYkZoh9KSiNNmqC8),older(144))))"
}

/// `wsh` descriptor with policy `and(pk(A),after(100000))`
pub fn get_test_single_sig_cltv() -> &'static str {
    "wsh(and_v(v:pk(cVpPVruEDdmutPzisEsYvtST1usBR3ntr8pXSyt6D2YYqXRyPcFW),after(100000)))"
}

/// `wsh` descriptor with policy `and(pk(A),after(1_734_230_218))`
// the parameter passed to miniscript fragment `after` has to equal or greater than 500_000_000
// in order to use a lock based on unix time
pub fn get_test_single_sig_cltv_timestamp() -> &'static str {
    "wsh(and_v(v:pk(cVpPVruEDdmutPzisEsYvtST1usBR3ntr8pXSyt6D2YYqXRyPcFW),after(1734230218)))"
}

/// taproot single key descriptor
pub fn get_test_tr_single_sig() -> &'static str {
    "tr(cNJmN3fH9DDbDt131fQNkVakkpzawJBSeybCUNmP1BovpmGQ45xG)"
}

/// taproot descriptor with taptree
pub fn get_test_tr_with_taptree() -> &'static str {
    "tr(b511bd5771e47ee27558b1765e87b541668304ec567721c7b880edc0a010da55,{pk(cPZzKuNmpuUjD1e8jUU4PVzy2b5LngbSip8mBsxf4e7rSFZVb4Uh),pk(8aee2b8120a5f157f1223f72b5e62b825831a27a9fdf427db7cc697494d4a642)})"
}

/// taproot descriptor with private key taptree
pub fn get_test_tr_with_taptree_both_priv() -> &'static str {
    "tr(b511bd5771e47ee27558b1765e87b541668304ec567721c7b880edc0a010da55,{pk(cPZzKuNmpuUjD1e8jUU4PVzy2b5LngbSip8mBsxf4e7rSFZVb4Uh),pk(cNaQCDwmmh4dS9LzCgVtyy1e1xjCJ21GUDHe9K98nzb689JvinGV)})"
}

/// taproot descriptor where one key appears in two script paths
pub fn get_test_tr_repeated_key() -> &'static str {
    "tr(b511bd5771e47ee27558b1765e87b541668304ec567721c7b880edc0a010da55,{and_v(v:pk(cVpPVruEDdmutPzisEsYvtST1usBR3ntr8pXSyt6D2YYqXRyPcFW),after(100)),and_v(v:pk(cVpPVruEDdmutPzisEsYvtST1usBR3ntr8pXSyt6D2YYqXRyPcFW),after(200))})"
}

/// taproot xpriv descriptor
pub fn get_test_tr_single_sig_xprv() -> &'static str {
    "tr(tprv8ZgxMBicQKsPdDArR4xSAECuVxeX1jwwSXR4ApKbkYgZiziDc4LdBy2WvJeGDfUSE4UT4hHhbgEwbdq8ajjUHiKDegkwrNU6V55CxcxonVN/*)"
}

/// taproot xpriv and change descriptor
pub fn get_test_tr_single_sig_xprv_and_change_desc() -> (&'static str, &'static str) {
    ("tr(tprv8ZgxMBicQKsPdDArR4xSAECuVxeX1jwwSXR4ApKbkYgZiziDc4LdBy2WvJeGDfUSE4UT4hHhbgEwbdq8ajjUHiKDegkwrNU6V55CxcxonVN/0/*)",
    "tr(tprv8ZgxMBicQKsPdDArR4xSAECuVxeX1jwwSXR4ApKbkYgZiziDc4LdBy2WvJeGDfUSE4UT4hHhbgEwbdq8ajjUHiKDegkwrNU6V55CxcxonVN/1/*)")
}

/// taproot descriptor with taptree
pub fn get_test_tr_with_taptree_xprv() -> &'static str {
    "tr(cNJmN3fH9DDbDt131fQNkVakkpzawJBSeybCUNmP1BovpmGQ45xG,{pk(tprv8ZgxMBicQKsPdDArR4xSAECuVxeX1jwwSXR4ApKbkYgZiziDc4LdBy2WvJeGDfUSE4UT4hHhbgEwbdq8ajjUHiKDegkwrNU6V55CxcxonVN/*),pk(8aee2b8120a5f157f1223f72b5e62b825831a27a9fdf427db7cc697494d4a642)})"
}

/// taproot descriptor with duplicate script paths
pub fn get_test_tr_dup_keys() -> &'static str {
    "tr(cNJmN3fH9DDbDt131fQNkVakkpzawJBSeybCUNmP1BovpmGQ45xG,{pk(8aee2b8120a5f157f1223f72b5e62b825831a27a9fdf427db7cc697494d4a642),pk(8aee2b8120a5f157f1223f72b5e62b825831a27a9fdf427db7cc697494d4a642)})"
}

/// A new empty transaction with the given locktime
pub fn new_tx(locktime: u32) -> Transaction {
    Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::from_consensus(locktime),
        input: vec![],
        output: vec![],
    }
}

/// Construct a new [`FeeRate`] from the given raw `sat_vb` feerate. This is
/// useful in cases where we want to create a feerate from a `f64`, as the
/// traditional [`FeeRate::from_sat_per_vb`] method will only accept an integer.
///
/// **Note** this 'quick and dirty' conversion should only be used when the input
/// parameter has units of `satoshis/vbyte` **AND** is not expected to overflow,
/// or else the resulting value will be inaccurate.
pub fn feerate_unchecked(sat_vb: f64) -> FeeRate {
    // 1 sat_vb / 4wu_vb * 1000kwu_wu = 250 sat_kwu
    let sat_kwu = (sat_vb * 250.0).ceil() as u64;
    FeeRate::from_sat_per_kwu(sat_kwu)
}

/// Input parameter for [`receive_output`].
pub enum ReceiveTo {
    /// Receive tx to mempool at this `last_seen` timestamp.
    Mempool(u64),
    /// Receive tx to block with this anchor.
    Block(ConfirmationBlockTime),
}

impl From<ConfirmationBlockTime> for ReceiveTo {
    fn from(value: ConfirmationBlockTime) -> Self {
        Self::Block(value)
    }
}

/// Receive a tx output with the given value in the latest block
pub fn receive_output_in_latest_block(wallet: &mut Wallet, value: Amount) -> OutPoint {
    let latest_cp = wallet.latest_checkpoint();
    let height = latest_cp.height();
    assert!(height > 0, "cannot receive tx into genesis block");
    receive_output(
        wallet,
        value,
        ConfirmationBlockTime {
            block_id: latest_cp.block_id(),
            confirmation_time: 0,
        },
    )
}

/// Receive a tx output with the given value and chain position
pub fn receive_output(
    wallet: &mut Wallet,
    value: Amount,
    receive_to: impl Into<ReceiveTo>,
) -> OutPoint {
    let addr = wallet.next_unused_address(KeychainKind::External).address;
    receive_output_to_address(wallet, addr, value, receive_to)
}

/// Receive a tx output to an address with the given value and chain position
pub fn receive_output_to_address(
    wallet: &mut Wallet,
    addr: Address,
    value: Amount,
    receive_to: impl Into<ReceiveTo>,
) -> OutPoint {
    let tx = Transaction {
        version: transaction::Version::ONE,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![TxOut {
            script_pubkey: addr.script_pubkey(),
            value,
        }],
    };

    let txid = tx.compute_txid();
    insert_tx(wallet, tx);

    match receive_to.into() {
        ReceiveTo::Block(anchor) => insert_anchor(wallet, txid, anchor),
        ReceiveTo::Mempool(last_seen) => insert_seen_at(wallet, txid, last_seen),
    }

    OutPoint { txid, vout: 0 }
}

/// Insert a checkpoint into the wallet. This can be used to extend the wallet's local chain
/// or to insert a block that did not exist previously. Note that if replacing a block with
/// a different one at the same height, then all later blocks are evicted as well.
pub fn insert_checkpoint(wallet: &mut Wallet, block: BlockId) {
    let mut cp = wallet.latest_checkpoint();
    cp = cp.insert(block);
    wallet
        .apply_update(Update {
            chain: Some(cp),
            ..Default::default()
        })
        .unwrap();
}

/// Inserts a transaction into the local view, assuming it is currently present in the mempool.
///
/// This can be used, for example, to track a transaction immediately after it is broadcast.
pub fn insert_tx(wallet: &mut Wallet, tx: Transaction) {
    let txid = tx.compute_txid();
    let seen_at = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs();
    let mut tx_update = TxUpdate::default();
    tx_update.txs = vec![Arc::new(tx)];
    tx_update.seen_ats = [(txid, seen_at)].into();
    wallet
        .apply_update(Update {
            tx_update,
            ..Default::default()
        })
        .expect("failed to apply update");
}

/// Simulates confirming a tx with `txid` by applying an update to the wallet containing
/// the given `anchor`. Note: to be considered confirmed the anchor block must exist in
/// the current active chain.
pub fn insert_anchor(wallet: &mut Wallet, txid: Txid, anchor: ConfirmationBlockTime) {
    let mut tx_update = TxUpdate::default();
    tx_update.anchors = [(anchor, txid)].into();
    wallet
        .apply_update(Update {
            tx_update,
            ..Default::default()
        })
        .expect("failed to apply update");
}

/// Marks the given `txid` seen as unconfirmed at `seen_at`
pub fn insert_seen_at(wallet: &mut Wallet, txid: Txid, seen_at: u64) {
    let mut tx_update = TxUpdate::default();
    tx_update.seen_ats = [(txid, seen_at)].into();
    wallet
        .apply_update(Update {
            tx_update,
            ..Default::default()
        })
        .expect("failed to apply update");
}

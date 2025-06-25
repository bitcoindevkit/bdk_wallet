use std::{
    cmp,
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    sync::Arc,
};

use bdk_wallet::{
    bitcoin::{
        self, absolute::LockTime, hashes::Hash, transaction::Version, Amount, BlockHash, OutPoint,
        Transaction, TxIn, TxOut, Txid,
    },
    chain::{BlockId, CheckPoint, ConfirmationBlockTime},
    KeychainKind, Wallet,
};

use crate::fuzzed_data_provider::{
    consume_bool, consume_bytes, consume_u32, consume_u64, consume_u8,
};

pub fn consume_block_hash(data: &mut &[u8]) -> BlockHash {
    let bytes: [u8; 32] = match consume_bytes(data, 32).try_into() {
        Ok(bytes) => bytes,
        Err(_) => [0; 32],
    };

    BlockHash::from_byte_array(bytes)
}

pub fn consume_txid(data: &mut &[u8]) -> Txid {
    let bytes: [u8; 32] = match consume_bytes(data, 32).try_into() {
        Ok(bytes) => bytes,
        Err(_) => [0; 32],
    };

    Txid::from_byte_array(bytes)
}

pub fn consume_keychain_indices(
    data: &mut &[u8],
    keychain: KeychainKind,
) -> BTreeMap<KeychainKind, u32> {
    let mut indices = BTreeMap::new();
    if consume_bool(data) {
        let count = consume_u8(data) as u32;
        let start = consume_u8(data) as u32;
        indices.extend((start..count).map(|idx| (keychain, idx)))
    }
    indices
}

// TODO: (@leonardo) improve this implementation to not rely on UniqueHash
pub fn consume_spk(data: &mut &[u8], wallet: &mut Wallet) -> bitcoin::ScriptBuf {
    if data.is_empty() {
        let bytes = consume_bytes(data, 32);
        return bitcoin::ScriptBuf::from_bytes(bytes);
    }

    let flags = data[0];
    *data = &data[1..];

    match flags.trailing_zeros() {
        0 => wallet
            .next_unused_address(KeychainKind::External)
            .script_pubkey(),
        1 => wallet
            .next_unused_address(KeychainKind::Internal)
            .script_pubkey(),
        _ => {
            let bytes = consume_bytes(data, 32);
            bitcoin::ScriptBuf::from_bytes(bytes)
        }
    }
}

// TODO: (@leonardo) improve this implementation to not rely on UniqueHash
pub fn consume_txs(mut data: &[u8], wallet: &mut Wallet) -> Vec<Arc<Transaction>> {
    // TODO: (@leonardo) should this be a usize ?

    let count = consume_u8(&mut data);
    let mut txs = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let version = consume_u32(&mut data);
        // TODO: (@leonardo) should we use the Version::consensus_decode instead ?
        let version = Version(version as i32);

        let locktime = consume_u32(&mut data);
        let locktime = LockTime::from_consensus(locktime);

        let txin_count = consume_u8(&mut data);
        let mut tx_inputs = Vec::with_capacity(txin_count as usize);

        for _ in 0..txin_count {
            let prev_txid = consume_txid(&mut data);
            let prev_vout = consume_u32(&mut data);
            let prev_output = OutPoint::new(prev_txid, prev_vout);
            let tx_input = TxIn {
                previous_output: prev_output,
                ..Default::default()
            };
            tx_inputs.push(tx_input);
        }

        let txout_count = consume_u8(&mut data);
        let mut tx_outputs = Vec::with_capacity(txout_count as usize);

        for _ in 0..txout_count {
            let spk = consume_spk(&mut data, wallet);
            let sats = (consume_u8(&mut data) as u64) * 1_000;
            let amount = Amount::from_sat(sats);
            let tx_output = TxOut {
                value: amount,
                script_pubkey: spk,
            };
            tx_outputs.push(tx_output);
        }

        let tx = Transaction {
            version,
            lock_time: locktime,
            input: tx_inputs,
            output: tx_outputs,
        };

        txs.push(tx.into());
    }
    txs
}

pub fn consume_txouts(mut data: &[u8]) -> BTreeMap<OutPoint, TxOut> {
    // TODO: (@leonardo) should this be a usize ?
    let count = consume_u8(&mut data);
    let mut txouts = BTreeMap::new();
    for _ in 0..count {
        let prev_txid = consume_txid(&mut data);
        let prev_vout = consume_u32(&mut data);
        let prev_output = OutPoint::new(prev_txid, prev_vout);

        let sats = (consume_u8(&mut data) as u64) * 1_000;
        let amount = Amount::from_sat(sats);

        // TODO: (@leonardo) should we use different spks ?
        let txout = TxOut {
            value: amount,
            script_pubkey: Default::default(),
        };

        txouts.insert(prev_output, txout);
    }
    txouts
}

pub fn consume_anchors(
    mut data: &[u8],
    mut unconfirmed_txids: VecDeque<Txid>,
) -> BTreeSet<(ConfirmationBlockTime, Txid)> {
    let mut anchors = BTreeSet::new();

    let count = consume_u8(&mut data);
    // FIXME: (@leonardo) should we use while limited by a flag instead ? (as per antoine's impls)
    for _ in 0..count {
        let block_height = consume_u32(&mut data);
        let block_hash = consume_block_hash(&mut data);

        let block_id = BlockId {
            height: block_height,
            hash: block_hash,
        };

        let confirmation_time = consume_u64(&mut data);

        let anchor = ConfirmationBlockTime {
            block_id,
            confirmation_time,
        };

        if let Some(txid) = unconfirmed_txids.pop_front() {
            anchors.insert((anchor, txid));
        } else {
            break;
        }
    }
    anchors
}

pub fn consume_seen_ats(
    mut data: &[u8],
    mut unconfirmed_txids: VecDeque<Txid>,
) -> HashSet<(Txid, u64)> {
    let mut seen_ats = HashSet::new();

    let count = consume_u8(&mut data);
    // FIXME: (@leonardo) should we use while limited by a flag instead ? (as per antoine's impls)
    for _ in 0..count {
        let time = cmp::min(consume_u64(&mut data), i64::MAX as u64 - 1);

        if let Some(txid) = unconfirmed_txids.pop_front() {
            seen_ats.insert((txid, time));
        } else {
            let txid = consume_txid(&mut data);
            seen_ats.insert((txid, time));
        }
    }
    seen_ats
}

pub fn consume_evicted_ats(
    mut data: &[u8],
    mut unconfirmed_txids: VecDeque<Txid>,
) -> HashSet<(Txid, u64)> {
    let mut evicted_at = HashSet::new();

    let count = consume_u8(&mut data);
    // FIXME: (@leonardo) should we use while limited by a flag instead ? (as per antoine's impls)
    for _ in 0..count {
        let time = cmp::min(consume_u64(&mut data), i64::MAX as u64 - 1);
        if let Some(txid) = unconfirmed_txids.pop_front() {
            evicted_at.insert((txid, time));
        } else {
            let txid = consume_txid(&mut data);
            evicted_at.insert((txid, time));
        }
    }

    evicted_at
}

pub fn consume_checkpoint(mut data: &[u8], wallet: &mut Wallet) -> CheckPoint {
    let mut tip = wallet.latest_checkpoint();

    let _tip_hash = tip.hash();
    let tip_height = tip.height();

    let count = consume_u8(&mut data);
    // FIXME: (@leonardo) should we use while limited by a flag instead ? (as per antoine's impls)
    for i in 1..count {
        let height = tip_height + i as u32;
        let hash = consume_block_hash(&mut data);

        let block_id = BlockId { height, hash };

        tip = tip.push(block_id).unwrap();
    }
    tip
}

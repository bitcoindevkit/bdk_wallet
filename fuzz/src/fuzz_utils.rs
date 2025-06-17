use bdk_wallet::{
    bitcoin::{self, hashes::Hash, BlockHash, Txid},
    KeychainKind, Wallet,
};

use crate::fuzzed_data_provider::consume_bytes;

#[macro_export]
macro_rules! try_consume_txs {
    ($data:expr, $wallet:expr) => {{
        let mut data_iter = $data.into_iter();

        let txs_count = try_consume_u8!(data_iter) as usize;
        let mut txs = Vec::with_capacity(txs_count);

        for _ in 0..txs_count {
            let version = try_consume_u32!(data_iter);
            let version = Version(version as i32);

            let lock_time = try_consume_u32!(data_iter);
            let lock_time = LockTime::from_consensus(lock_time);

            let txin_count = try_consume_u8!(data_iter) as usize;
            let mut input = Vec::with_capacity(txin_count);

            for _ in 0..txin_count {
                let prev_txid = consume_txid($data);
                let prev_vout = try_consume_u32!(data_iter);
                let prev_output = OutPoint::new(prev_txid, prev_vout);
                let tx_input = TxIn {
                    previous_output: prev_output,
                    ..Default::default()
                };
                input.push(tx_input);
            }

            let txout_count = try_consume_u8!(data_iter) as usize;
            let mut output = Vec::with_capacity(txout_count);

            for _ in 0..txout_count {
                let spk = consume_spk($data, $wallet);
                let sats = (try_consume_u8!(data_iter) as u64) * 1_000;
                let amount = Amount::from_sat(sats);
                let tx_output = TxOut {
                    value: amount,
                    script_pubkey: spk,
                };
                output.push(tx_output);
            }

            let tx = Transaction {
                version,
                lock_time,
                input,
                output,
            };

            txs.push(tx.into());
        }
        txs
    }};
}

#[macro_export]
macro_rules! try_consume_txouts {
    ($data:expr) => {{
        let mut data_iter = $data.into_iter();
        let mut txouts = BTreeMap::new();

        let txouts_count = try_consume_u8!(data_iter);
        for _ in 0..txouts_count {
            let prev_txid = consume_txid($data);
            let prev_vout = try_consume_u32!(data_iter);
            let prev_output = OutPoint::new(prev_txid, prev_vout);

            let sats = (try_consume_u8!(data_iter) as u64) * 1_000;
            let amount = Amount::from_sat(sats);

            // TODO: (@leonardo) should it use fuzzed spks ?
            let txout = TxOut {
                value: amount,
                script_pubkey: Default::default(),
            };

            txouts.insert(prev_output, txout);
        }
        txouts
    }};
}

#[macro_export]
macro_rules! try_consume_anchors {
    ($data:expr, $unconfirmed_txids:expr) => {{
        let mut data_iter = $data.into_iter();
        let mut anchors = BTreeSet::new();

        let count = try_consume_u8!(data_iter);
        for _ in 0..count {
            let block_height = try_consume_u32!(data_iter);
            let block_hash = consume_block_hash($data);

            let block_id = BlockId {
                height: block_height,
                hash: block_hash,
            };

            let confirmation_time = try_consume_u64!(data_iter);

            let anchor = ConfirmationBlockTime {
                block_id,
                confirmation_time,
            };

            if let Some(txid) = $unconfirmed_txids.pop_front() {
                anchors.insert((anchor, txid));
            } else {
                break;
            }
        }
        anchors
    }};
}

#[macro_export]
macro_rules! try_consume_seen_or_evicted_ats {
    ($data:expr, $unconfirmed_txids:expr) => {{
        let mut data_iter = $data.into_iter();
        let mut seen_or_evicted_ats = HashSet::new();

        let count = try_consume_u8!(data_iter);
        for _ in 0..count {
            let time = cmp::min(try_consume_u64!(data_iter), i64::MAX as u64 - 1);

            if let Some(txid) = $unconfirmed_txids.pop_front() {
                seen_or_evicted_ats.insert((txid, time));
            } else {
                let txid = consume_txid($data);
                seen_or_evicted_ats.insert((txid, time));
            }
        }
        seen_or_evicted_ats
    }};
}

#[macro_export]
macro_rules! try_consume_checkpoint {
    ($data:expr, $wallet:expr) => {{
        let mut data_iter = $data.into_iter();

        let mut tip = $wallet.latest_checkpoint();
        let _tip_hash = tip.hash();
        let tip_height = tip.height();

        let count = try_consume_u8!(data_iter);
        // FIXME: (@leonardo) should we use while limited by a flag instead ? (as per antoine's
        // impls)
        for i in 1..count {
            let height = tip_height + i as u32;
            let hash = consume_block_hash($data);

            let block_id = BlockId { height, hash };

            tip = tip.push(block_id).unwrap();
        }
        tip
    }};
}

pub fn consume_txid(data: &mut &[u8]) -> Txid {
    let bytes: [u8; 32] = consume_bytes(data, 32).try_into().unwrap_or([0; 32]);

    Txid::from_byte_array(bytes)
}

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

pub fn consume_block_hash(data: &mut &[u8]) -> BlockHash {
    let bytes: [u8; 32] = consume_bytes(data, 32).try_into().unwrap_or([0; 32]);

    BlockHash::from_byte_array(bytes)
}

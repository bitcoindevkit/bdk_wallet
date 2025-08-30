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

#[macro_export]
macro_rules! try_consume_sign_options {
    ($data_iter:expr) => {{
        let mut sign_options = SignOptions::default();

        if try_consume_bool!($data_iter) {
            sign_options.trust_witness_utxo = true;
        }

        if try_consume_bool!($data_iter) {
            let height = try_consume_u32!($data_iter);
            sign_options.assume_height = Some(height);
        }

        if try_consume_bool!($data_iter) {
            sign_options.allow_all_sighashes = true;
        }

        if try_consume_bool!($data_iter) {
            sign_options.try_finalize = false;
        }

        if try_consume_bool!($data_iter) {
            // FIXME: how can we use the other include/exclude variants here ?
            if try_consume_bool!($data_iter) {
                sign_options.tap_leaves_options = TapLeavesOptions::All;
            } else {
                sign_options.tap_leaves_options = TapLeavesOptions::None;
            }
        }

        if try_consume_bool!($data_iter) {
            sign_options.sign_with_tap_internal_key = false;
        }

        if try_consume_bool!($data_iter) {
            sign_options.allow_grinding = false;
        }

        sign_options
    }};
}

#[macro_export]
macro_rules! try_consume_tx_builder {
    ($data:expr, $wallet:expr) => {{
        let mut data_iter = $data.into_iter();

        let utxo = $wallet.list_unspent().next();

        let recipients_count = *try_consume_byte!(data_iter) as usize;
        let mut recipients = Vec::with_capacity(recipients_count);
        for _ in 0..recipients_count {
            let spk = consume_spk($data, $wallet);
            let amount = *try_consume_byte!(data_iter) as u64 * 1_000;
            let amount = bitcoin::Amount::from_sat(amount);
            recipients.push((spk, amount));
        }

        let drain_to = consume_spk($data, $wallet);

        let mut tx_builder = match try_consume_bool!(data_iter) {
            true => $wallet.build_tx(),
            false => {
                // FIXME: (@leonardo) get a randomized txid.
                let txid = $wallet
                    .tx_graph()
                    .full_txs()
                    .next()
                    .map(|tx_node| tx_node.txid);
                match txid {
                    Some(txid) => match $wallet.build_fee_bump(txid) {
                        Ok(builder) => builder,
                        Err(_) => continue,
                    },
                    None => continue,
                }
            }
        };

        if try_consume_bool!(data_iter) {
            let mut rate = *try_consume_byte!(data_iter) as u64;
            if try_consume_bool!(data_iter) {
                rate *= 1_000;
            }
            let rate =
                bitcoin::FeeRate::from_sat_per_vb(rate).expect("It should be a valid fee rate.");
            tx_builder.fee_rate(rate);
        }

        if try_consume_bool!(data_iter) {
            let mut fee = *try_consume_byte!(data_iter) as u64;
            if try_consume_bool!(data_iter) {
                fee *= 1_000;
            }
            let fee = bitcoin::Amount::from_sat(fee);
            tx_builder.fee_absolute(fee);
        }

        if try_consume_bool!(data_iter) {
            if let Some(ref utxo) = utxo {
                tx_builder
                    .add_utxo(utxo.outpoint)
                    .expect("It should be a known UTXO.");
            }
        }

        // FIXME: add the fuzzed option for `TxBuilder.add_foreign_utxo`.

        if try_consume_bool!(data_iter) {
            tx_builder.manually_selected_only();
        }

        if try_consume_bool!(data_iter) {
            if let Some(ref utxo) = utxo {
                tx_builder.add_unspendable(utxo.outpoint);
            }
        }

        if try_consume_bool!(data_iter) {
            let sighash =
                bitcoin::psbt::PsbtSighashType::from_u32(*try_consume_byte!(data_iter) as u32);
            tx_builder.sighash(sighash);
        }

        if try_consume_bool!(data_iter) {
            let ordering = if try_consume_bool!(data_iter) {
                TxOrdering::Shuffle
            } else {
                TxOrdering::Untouched
            };
            tx_builder.ordering(ordering);
        }

        if try_consume_bool!(data_iter) {
            let lock_time = try_consume_u32!(data_iter);
            let lock_time = bitcoin::absolute::LockTime::from_consensus(lock_time);
            tx_builder.nlocktime(lock_time);
        }

        if try_consume_bool!(data_iter) {
            let version = try_consume_u32!(data_iter);
            tx_builder.version(version as i32);
        }

        if try_consume_bool!(data_iter) {
            tx_builder.do_not_spend_change();
        }

        if try_consume_bool!(data_iter) {
            tx_builder.only_spend_change();
        }

        if try_consume_bool!(data_iter) {
            tx_builder.only_witness_utxo();
        }

        if try_consume_bool!(data_iter) {
            tx_builder.include_output_redeem_witness_script();
        }

        if try_consume_bool!(data_iter) {
            tx_builder.add_global_xpubs();
        }

        if try_consume_bool!(data_iter) {
            tx_builder.drain_wallet();
        }

        if try_consume_bool!(data_iter) {
            tx_builder.allow_dust(true);
        }

        if try_consume_bool!(data_iter) {
            tx_builder.set_recipients(recipients);
        }

        // FIXME: add the fuzzed option for `TxBuilder.add_data()` method.

        if try_consume_bool!(data_iter) {
            tx_builder.drain_to(drain_to);
        }

        tx_builder
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

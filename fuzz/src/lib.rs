use bdk_wallet::{
    bitcoin::{absolute::LockTime, hashes::Hash, transaction::Version, Amount, BlockHash, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid},
    chain::{BlockId, CheckPoint, ConfirmationBlockTime, TxUpdate},
    KeychainKind, Update, Wallet,
};
use std::{collections::BTreeMap, sync::Arc};

pub const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
pub const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
pub const NETWORK: bdk_wallet::bitcoin::Network = bdk_wallet::bitcoin::Network::Regtest;

/// Creates a fresh wallet (no persistence) using deterministic test descriptors.
pub fn create_wallet() -> Wallet {
    Wallet::create(EXTERNAL_DESC, INTERNAL_DESC)
        .network(NETWORK)
        .create_wallet_no_persist()
        .expect("valid descriptors")
}

/// Builds a minimal valid P2WPKH output script from a 20-byte hash.
pub fn p2wpkh_script(hash: [u8; 20]) -> ScriptBuf {
    let mut v = vec![0x00u8, 0x14u8]; // OP_0 PUSH20
    v.extend_from_slice(&hash);
    ScriptBuf::from_bytes(v)
}

/// Builds an `Update` that funds the wallet at external index 0 with `value` sats,
/// confirmed at height `height`. Used by `create_tx` to seed UTXOs.
pub fn funding_update(wallet: &Wallet, value: u64, height: u32) -> Update {
    let receive_script = wallet
        .peek_address(KeychainKind::External, 0)
        .script_pubkey();

    let tx = Transaction {
        version: Version::ONE,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: Txid::from_byte_array([0xab; 32]),
                vout: 0,
            },
            sequence: Sequence::MAX,
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(value),
            script_pubkey: receive_script,
        }],
    };

    let txid = tx.compute_txid();
    let genesis_hash = wallet.latest_checkpoint().hash();

    let mut tx_update = TxUpdate::<ConfirmationBlockTime>::default();
    tx_update.txs = vec![Arc::new(tx)];
    tx_update.anchors.insert((
        ConfirmationBlockTime {
            block_id: BlockId { height, hash: BlockHash::all_zeros() },
            confirmation_time: 1_600_000_000,
        },
        txid,
    ));

    let chain = CheckPoint::from_block_ids([
        BlockId { height: 0, hash: genesis_hash },
        BlockId { height, hash: BlockHash::all_zeros() },
    ])
    .ok();

    Update {
        tx_update,
        chain,
        last_active_indices: BTreeMap::from([(KeychainKind::External, 0)]),
    }
}

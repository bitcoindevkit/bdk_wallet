// Bitcoin Dev Kit
// Written in 2020 by Alekos Filini <alekos.filini@gmail.com>
//
// Copyright (c) 2020-2021 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use bitcoin::secp256k1::{All, Secp256k1};
use bitcoin::{
    absolute, relative, Amount, FeeRate, Script, Sequence, SignedAmount, Transaction, Txid,
};
use chain::{ChainPosition, ConfirmationBlockTime};
use core::fmt;
use miniscript::{MiniscriptKey, Satisfier, ToPublicKey};
use rand_core::RngCore;
use serde::de::{self, Visitor};
use serde::Deserializer;

/// Trait to check if a value is below the dust limit.
/// We are performing dust value calculation for a given script public key using rust-bitcoin to
/// keep it compatible with network dust rate
// we implement this trait to make sure we don't mess up the comparison with off-by-one like a <
// instead of a <= etc.
pub trait IsDust {
    /// Check whether or not a value is below dust limit
    fn is_dust(&self, script: &Script) -> bool;
}

impl IsDust for Amount {
    fn is_dust(&self, script: &Script) -> bool {
        *self < script.minimal_non_dust()
    }
}

impl IsDust for u64 {
    fn is_dust(&self, script: &Script) -> bool {
        Amount::from_sat(*self).is_dust(script)
    }
}

pub struct After {
    pub current_height: Option<u32>,
    pub assume_height_reached: bool,
}

impl After {
    pub(crate) fn new(current_height: Option<u32>, assume_height_reached: bool) -> After {
        After {
            current_height,
            assume_height_reached,
        }
    }
}

pub(crate) fn check_nsequence_rbf(sequence: Sequence, csv: Sequence) -> bool {
    // The nSequence value must enable relative timelocks
    if !sequence.is_relative_lock_time() {
        return false;
    }

    // Both values should be represented in the same unit (either time-based or
    // block-height based)
    if sequence.is_time_locked() != csv.is_time_locked() {
        return false;
    }

    // The value should be at least `csv`
    if sequence < csv {
        return false;
    }

    true
}

impl<Pk: MiniscriptKey + ToPublicKey> Satisfier<Pk> for After {
    fn check_after(&self, n: absolute::LockTime) -> bool {
        if let Some(current_height) = self.current_height {
            current_height >= n.to_consensus_u32()
        } else {
            self.assume_height_reached
        }
    }
}

pub struct Older {
    pub current_height: Option<u32>,
    pub create_height: Option<u32>,
    pub assume_height_reached: bool,
}

impl Older {
    pub(crate) fn new(
        current_height: Option<u32>,
        create_height: Option<u32>,
        assume_height_reached: bool,
    ) -> Older {
        Older {
            current_height,
            create_height,
            assume_height_reached,
        }
    }
}

impl<Pk: MiniscriptKey + ToPublicKey> Satisfier<Pk> for Older {
    fn check_older(&self, n: relative::LockTime) -> bool {
        if let Some(current_height) = self.current_height {
            // TODO: test >= / >
            current_height
                >= self
                    .create_height
                    .unwrap_or(0)
                    .checked_add(n.to_consensus_u32())
                    .expect("Overflowing addition")
        } else {
            self.assume_height_reached
        }
    }
}

// The Knuth shuffling algorithm based on the original [Fisher-Yates method](https://en.wikipedia.org/wiki/Fisher%E2%80%93Yates_shuffle)
pub(crate) fn shuffle_slice<T>(list: &mut [T], rng: &mut impl RngCore) {
    if list.is_empty() {
        return;
    }
    let mut current_index = list.len() - 1;
    while current_index > 0 {
        let random_index = rng.next_u32() as usize % (current_index + 1);
        list.swap(current_index, random_index);
        current_index -= 1;
    }
}

pub(crate) type SecpCtx = Secp256k1<All>;

/// Details about a transaction affecting the wallet (relevant and canonical).
#[derive(Debug)]
pub struct TxDetails {
    /// The transaction id.
    pub txid: Txid,
    /// The sum of the transaction input amounts that spend from previous outputs tracked by this
    /// wallet.
    pub sent: Amount,
    /// The sum of the transaction outputs that send to script pubkeys tracked by this wallet.
    pub received: Amount,
    /// The fee paid for the transaction. Note that to calculate the fee for a transaction with
    /// inputs not owned by this wallet you must manually insert the TxOut(s) into the tx graph
    /// using the insert_txout function. If those are not available, the field will be `None`.
    pub fee: Option<Amount>,
    /// The fee rate paid for the transaction. Note that to calculate the fee rate for a
    /// transaction with inputs not owned by this wallet you must manually insert the TxOut(s) into
    /// the tx graph using the insert_txout function. If those are not available, the field will be
    /// `None`.
    pub fee_rate: Option<FeeRate>,
    /// The net effect of the transaction on the balance of the wallet.
    pub balance_delta: SignedAmount,
    /// The position of the transaction in the chain.
    pub chain_position: ChainPosition<ConfirmationBlockTime>,
    /// The complete [`Transaction`].
    pub tx: Arc<Transaction>,
}

/// Validate ISO-8601 time string format (basic YYYY-MM-DDThh:mm:ss check).
pub fn validate_iso8601(s: &str) -> Result<(), String> {
    // Basic ISO 8601 check: YYYY-MM-DDThh:mm:ss[Z|(+|-)hh:mm]
    // 2023-01-01T00:00:00Z -> len 20
    if s.len() < 19 {
        return Err("ISO-8601 time string too short".into());
    }

    // Check separators
    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return Err("Invalid date separators".into());
    }
    if bytes[10] != b'T' && bytes[10] != b' ' {
        // Allow ' ' or 'T'
        return Err("Invalid time separator".into());
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return Err("Invalid time separators".into());
    }

    // Check digits
    for (i, b) in bytes.iter().enumerate() {
        if matches!(i, 4 | 7 | 10 | 13 | 16) {
            continue;
        }
        if i >= 19 {
            break;
        } // Check specific format part only, ignore timezone/fractional for basics
        if !b.is_ascii_digit() {
            return Err(alloc::format!("Invalid digit at position {}", i));
        }
    }

    Ok(())
}

/// Validate rate map currency codes and values.
pub fn validate_rate_map(m: &BTreeMap<String, f64>) -> Result<(), String> {
    for (currency, rate) in m {
        if *rate <= 0.0 {
            return Err(alloc::format!(
                "Rate logic error: {} has non-positive value",
                currency
            ));
        }
        // ISO 4217 check (basic 3 char check)
        if currency.len() != 3 {
            return Err(alloc::format!("Invalid currency code length: {}", currency));
        }
    }
    Ok(())
}

/// Leniently deserialize an Option<bool> from boolean, number, or string.
pub fn deserialize_option_boolsy<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    struct BoolsyVisitor;

    impl<'de> Visitor<'de> for BoolsyVisitor {
        type Value = Option<bool>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a boolean, number, or string representing a boolean")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v))
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v != 0))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v != 0))
        }

        fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v != 0.0))
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let s = v.trim().to_lowercase();
            match s.as_str() {
                "true" | "1" | "yes" | "y" => Ok(Some(true)),
                "false" | "0" | "no" | "n" | "" => Ok(Some(false)),
                _ => Err(E::custom(format!("invalid boolsy string: {}", v))),
            }
        }

        // Handle explicit null as false (per Python example)
        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(false))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(false))
        }

        fn visit_some<D>(self, d: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            d.deserialize_any(self)
        }
    }

    deserializer.deserialize_option(BoolsyVisitor)
}

#[cfg(test)]
mod test {
    // When nSequence is lower than this flag the timelock is interpreted as block-height-based,
    // otherwise it's time-based
    pub(crate) const SEQUENCE_LOCKTIME_TYPE_FLAG: u32 = 1 << 22;

    use super::{check_nsequence_rbf, shuffle_slice, IsDust};
    use crate::bitcoin::{Address, Network, Sequence};
    use alloc::vec::Vec;
    use core::str::FromStr;
    use rand::{rngs::StdRng, thread_rng, SeedableRng};

    #[test]
    fn test_is_dust() {
        let script_p2pkh = Address::from_str("1GNgwA8JfG7Kc8akJ8opdNWJUihqUztfPe")
            .unwrap()
            .require_network(Network::Bitcoin)
            .unwrap()
            .script_pubkey();
        assert!(script_p2pkh.is_p2pkh());
        assert!(545.is_dust(&script_p2pkh));
        assert!(!546.is_dust(&script_p2pkh));

        let script_p2wpkh = Address::from_str("bc1qxlh2mnc0yqwas76gqq665qkggee5m98t8yskd8")
            .unwrap()
            .require_network(Network::Bitcoin)
            .unwrap()
            .script_pubkey();
        assert!(script_p2wpkh.is_p2wpkh());
        assert!(293.is_dust(&script_p2wpkh));
        assert!(!294.is_dust(&script_p2wpkh));
    }

    #[test]
    fn test_check_nsequence_rbf_msb_set() {
        let result = check_nsequence_rbf(Sequence(0x80000000), Sequence(5000));
        assert!(!result);
    }

    #[test]
    fn test_check_nsequence_rbf_lt_csv() {
        let result = check_nsequence_rbf(Sequence(4000), Sequence(5000));
        assert!(!result);
    }

    #[test]
    fn test_check_nsequence_rbf_different_unit() {
        let result =
            check_nsequence_rbf(Sequence(SEQUENCE_LOCKTIME_TYPE_FLAG + 5000), Sequence(5000));
        assert!(!result);
    }

    #[test]
    fn test_check_nsequence_rbf_mask() {
        let result = check_nsequence_rbf(Sequence(0x3f + 10_000), Sequence(5000));
        assert!(result);
    }

    #[test]
    fn test_check_nsequence_rbf_same_unit_blocks() {
        let result = check_nsequence_rbf(Sequence(10_000), Sequence(5000));
        assert!(result);
    }

    #[test]
    fn test_check_nsequence_rbf_same_unit_time() {
        let result = check_nsequence_rbf(
            Sequence(SEQUENCE_LOCKTIME_TYPE_FLAG + 10_000),
            Sequence(SEQUENCE_LOCKTIME_TYPE_FLAG + 5000),
        );
        assert!(result);
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_shuffle_slice_empty_vec() {
        let mut test: Vec<u8> = vec![];
        shuffle_slice(&mut test, &mut thread_rng());
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_shuffle_slice_single_vec() {
        let mut test: Vec<u8> = vec![0];
        shuffle_slice(&mut test, &mut thread_rng());
    }

    #[test]
    fn test_shuffle_slice_duple_vec() {
        let seed = [0; 32];
        let mut rng: StdRng = SeedableRng::from_seed(seed);
        let mut test: Vec<u8> = vec![0, 1];
        shuffle_slice(&mut test, &mut rng);
        assert_eq!(test, &[0, 1]);
        let seed = [6; 32];
        let mut rng: StdRng = SeedableRng::from_seed(seed);
        let mut test: Vec<u8> = vec![0, 1];
        shuffle_slice(&mut test, &mut rng);
        assert_eq!(test, &[1, 0]);
    }

    #[test]
    fn test_shuffle_slice_multi_vec() {
        let seed = [0; 32];
        let mut rng: StdRng = SeedableRng::from_seed(seed);
        let mut test: Vec<u8> = vec![0, 1, 2, 4, 5];
        shuffle_slice(&mut test, &mut rng);
        assert_eq!(test, &[2, 1, 0, 4, 5]);
        let seed = [25; 32];
        let mut rng: StdRng = SeedableRng::from_seed(seed);
        let mut test: Vec<u8> = vec![0, 1, 2, 4, 5];
        shuffle_slice(&mut test, &mut rng);
        assert_eq!(test, &[0, 4, 1, 2, 5]);
    }
}

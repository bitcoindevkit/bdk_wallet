//! A validated set of keychains for constructing a [`Wallet`](crate::Wallet).
//!
//! A [`KeyRing`] pairs each keychain identifier `K` with the descriptor that keychain tracks, on a
//! single [`Network`]. It is a *construction-time* value: [`Wallet::create`](crate::Wallet::create)
//! consumes it, hands the descriptors to the wallet's indexer, and the `KeyRing` itself is not
//! retained.
//!
//! # Invariant
//!
//! A `KeyRing` is a proof that a valid `Wallet` can be constructed from it:
//!
//! - every descriptor parses and passes [`check_wallet_descriptor`]
//! - every descriptor matches the keyring's network
//! - each keychain maps to exactly one descriptor, and each descriptor to exactly one keychain
//! - at least one keychain exists
//!
//! Because [`KeyRing::new`] requires a descriptor, an empty keyring cannot be represented. All
//! validation happens in [`KeyRing::new`] and [`KeyRing::add_descriptor`]; there is no other way to
//! build one. Do not add a `Deserialize` implementation without validating on the way in — it would
//! reconstruct a `KeyRing` while skipping these checks.
//!
//! ```
//! # use bdk_wallet::keyring::KeyRing;
//! # use bdk_wallet::KeychainKind;
//! # use bitcoin::Network;
//! # const EXTERNAL: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
//! # const INTERNAL: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
//! let mut keyring = KeyRing::new(Network::Testnet, KeychainKind::External, EXTERNAL)?;
//! keyring.add_descriptor(KeychainKind::Internal, INTERNAL)?;
//!
//! assert_eq!(keyring.keychains().count(), 2);
//! # Ok::<(), bdk_wallet::keyring::KeyRingError<KeychainKind>>(())
//! ```
//!
//! [`check_wallet_descriptor`]: mod@crate::descriptor

pub mod error;

pub use error::KeyRingError;

use crate::collections::BTreeMap;
use crate::descriptor::{IntoWalletDescriptor, check_wallet_descriptor};
use crate::wallet::utils::SecpCtx;
use bitcoin::Network;
use bitcoin::secp256k1::Secp256k1;
use core::fmt::Debug;
use miniscript::{Descriptor, DescriptorPublicKey};

/// A validated set of keychains, used to construct a [`Wallet`](crate::Wallet).
///
/// See the [module documentation](self) for the invariant this type upholds.
#[derive(Debug)]
pub struct KeyRing<K> {
    secp: SecpCtx,
    network: Network,
    keychains: BTreeMap<K, Descriptor<DescriptorPublicKey>>,
}

impl<K> KeyRing<K>
where
    K: Ord + Clone + Debug,
{
    /// Create a `KeyRing` for `network` holding a single `keychain`.
    ///
    /// More keychains can be added with [`add_descriptor`](Self::add_descriptor). A keyring always
    /// holds at least one, so there is no "empty keyring" state to guard against later.
    ///
    /// # Errors
    ///
    /// If `descriptor` cannot be parsed, does not match `network`, or fails the checks applied to
    /// every wallet descriptor (multipath, hardened derivation in a public descriptor, miniscript
    /// sanity).
    pub fn new(
        network: Network,
        keychain: K,
        descriptor: impl IntoWalletDescriptor,
    ) -> Result<Self, KeyRingError<K>> {
        let secp = Secp256k1::new();
        let descriptor = Self::validate(&secp, network, descriptor)?;

        Ok(Self {
            secp,
            network,
            keychains: BTreeMap::from([(keychain, descriptor)]),
        })
    }

    /// Assign `descriptor` to `keychain`.
    ///
    /// # Errors
    ///
    /// As [`new`](Self::new), and additionally if `keychain` is already assigned a descriptor, or
    /// if `descriptor` is already assigned to another keychain. Both would make the wallet's
    /// indexer unable to attribute discovered outputs unambiguously.
    ///
    /// On error the keyring is left unchanged.
    pub fn add_descriptor(
        &mut self,
        keychain: K,
        descriptor: impl IntoWalletDescriptor,
    ) -> Result<(), KeyRingError<K>> {
        let descriptor = Self::validate(&self.secp, self.network, descriptor)?;

        if self.keychains.contains_key(&keychain) {
            return Err(KeyRingError::KeychainAlreadyAssigned(keychain));
        }
        if self.keychains.values().any(|d| d == &descriptor) {
            return Err(KeyRingError::DescriptorAlreadyAssigned(
                alloc::boxed::Box::new(descriptor),
            ));
        }

        self.keychains.insert(keychain, descriptor);
        Ok(())
    }

    /// The network these descriptors are valid for.
    pub fn network(&self) -> Network {
        self.network
    }

    /// Iterate over the keychains and their descriptors.
    ///
    /// # Ordering
    ///
    /// Keychains are yielded in the order defined by `K`'s [`Ord`] implementation. That order is
    /// stable across runs but otherwise *arbitrary* — for a derived `Ord` on an enum it is
    /// declaration order, so reordering variants changes what you see here. It carries no meaning;
    /// sort explicitly if a particular order matters.
    pub fn keychains(&self) -> impl Iterator<Item = (&K, &Descriptor<DescriptorPublicKey>)> {
        self.keychains.iter()
    }

    /// The descriptor assigned to `keychain`, if any.
    pub fn descriptor(&self, keychain: &K) -> Option<&Descriptor<DescriptorPublicKey>> {
        self.keychains.get(keychain)
    }

    /// Convert into the parameters used to create a [`Wallet`](crate::Wallet).
    ///
    /// Prefer [`Wallet::create`](crate::Wallet::create), which calls this for you.
    pub fn into_params(self) -> crate::CreateParams<K> {
        crate::CreateParams {
            secp: self.secp,
            descriptors: self.keychains,
            network: self.network,
            genesis_hash: None,
            lookahead: bdk_chain::keychain_txout::DEFAULT_LOOKAHEAD,
            use_spk_cache: false,
        }
    }

    fn validate(
        secp: &SecpCtx,
        network: Network,
        descriptor: impl IntoWalletDescriptor,
    ) -> Result<Descriptor<DescriptorPublicKey>, KeyRingError<K>> {
        let (descriptor, _keymap) = descriptor.into_wallet_descriptor(secp, network.into())?;
        check_wallet_descriptor(&descriptor)?;
        Ok(descriptor)
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod test {
    use super::*;
    use crate::KeychainKind;
    use alloc::vec;
    use alloc::vec::Vec;

    const EXTERNAL: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
    const INTERNAL: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";

    #[test]
    fn new_holds_one_keychain() {
        let keyring = KeyRing::new(Network::Testnet, KeychainKind::External, EXTERNAL).unwrap();
        assert_eq!(keyring.network(), Network::Testnet);
        assert_eq!(keyring.keychains().count(), 1);
        assert!(keyring.descriptor(&KeychainKind::External).is_some());
        assert!(keyring.descriptor(&KeychainKind::Internal).is_none());
    }

    #[test]
    fn add_descriptor_extends() {
        let mut keyring = KeyRing::new(Network::Testnet, KeychainKind::External, EXTERNAL).unwrap();
        keyring
            .add_descriptor(KeychainKind::Internal, INTERNAL)
            .unwrap();
        assert_eq!(keyring.keychains().count(), 2);
    }

    #[test]
    fn rejects_duplicate_keychain() {
        let mut keyring = KeyRing::new(Network::Testnet, KeychainKind::External, EXTERNAL).unwrap();
        assert!(matches!(
            keyring.add_descriptor(KeychainKind::External, INTERNAL),
            Err(KeyRingError::KeychainAlreadyAssigned(
                KeychainKind::External
            ))
        ));
        // unchanged on error
        assert_eq!(keyring.keychains().count(), 1);
    }

    #[test]
    fn rejects_duplicate_descriptor() {
        let mut keyring = KeyRing::new(Network::Testnet, KeychainKind::External, EXTERNAL).unwrap();
        assert!(matches!(
            keyring.add_descriptor(KeychainKind::Internal, EXTERNAL),
            Err(KeyRingError::DescriptorAlreadyAssigned(_))
        ));
        assert_eq!(keyring.keychains().count(), 1);
    }

    #[test]
    fn rejects_wrong_network() {
        // A testnet xpriv in a keyring declared for Bitcoin.
        assert!(matches!(
            KeyRing::new(Network::Bitcoin, KeychainKind::External, EXTERNAL),
            Err(KeyRingError::Descriptor(_))
        ));
    }

    #[test]
    fn rejects_multipath_descriptor() {
        const MULTIPATH: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/<0;1>/*)";
        assert!(matches!(
            KeyRing::new(Network::Testnet, KeychainKind::External, MULTIPATH),
            Err(KeyRingError::Descriptor(_))
        ));
    }

    #[test]
    fn keychains_iterate_in_ord_order() {
        let mut keyring = KeyRing::new(Network::Testnet, KeychainKind::Internal, INTERNAL).unwrap();
        keyring
            .add_descriptor(KeychainKind::External, EXTERNAL)
            .unwrap();
        // External < Internal by declaration order, regardless of insertion order.
        let order: Vec<_> = keyring.keychains().map(|(k, _)| *k).collect();
        assert_eq!(order, vec![KeychainKind::External, KeychainKind::Internal]);
    }
}

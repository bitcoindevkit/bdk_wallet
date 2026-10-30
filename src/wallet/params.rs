use crate::collections::BTreeMap;
use alloc::boxed::Box;

use bdk_chain::keychain_txout::DEFAULT_LOOKAHEAD;
use bitcoin::{BlockHash, Network, NetworkKind};
use miniscript::descriptor::KeyMap;

use bdk_chain::DescriptorExt;

use crate::{
    AsyncWalletPersister, CreateWithPersistError, LoadWithPersistError, Wallet, WalletPersister,
    descriptor::{
        DescriptorError, ExtendedDescriptor, IntoWalletDescriptor, check_wallet_descriptor,
    },
    utils::SecpCtx,
};

use super::{ChangeSet, CreateError, LoadError, PersistedWallet};

/// This atrocity is to avoid having type parameters on [`CreateParams`] and [`LoadParams`].
///
/// The better option would be to do `Box<dyn IntoWalletDescriptor>`, but we cannot due to Rust's
/// [object safety rules](https://doc.rust-lang.org/reference/items/traits.html#object-safety).
type DescriptorToExtract = Box<
    dyn FnOnce(&SecpCtx, NetworkKind) -> Result<(ExtendedDescriptor, KeyMap), DescriptorError>
        + Send
        + 'static,
>;

fn make_descriptor_to_extract<D>(descriptor: D) -> DescriptorToExtract
where
    D: IntoWalletDescriptor + Send + 'static,
{
    Box::new(|secp, network_kind| descriptor.into_wallet_descriptor(secp, network_kind))
}

/// Parameters for [`Wallet::create`] or [`PersistedWallet::create`].
///
/// Keychains are added one at a time with [`descriptor`](Self::descriptor). Each descriptor is
/// validated as it is added; the first failure is kept and returned when the wallet is created,
/// so the builder chain itself never fails.
#[must_use]
pub struct CreateParams<K: Ord> {
    pub(crate) secp: SecpCtx,
    pub(crate) descriptors: BTreeMap<K, ExtendedDescriptor>,
    pub(crate) network: Network,
    pub(crate) genesis_hash: Option<BlockHash>,
    pub(crate) lookahead: u32,
    pub(crate) use_spk_cache: bool,
    pub(crate) error: Option<CreateError<K>>,
}

impl<K: Ord + Clone + core::fmt::Debug> CreateParams<K> {
    /// Construct parameters for a wallet on `network`, with no keychains.
    ///
    /// Add keychains with [`descriptor`](Self::descriptor); at least one is required.
    ///
    /// Default values:
    /// * `genesis_hash` = `None`
    /// * `lookahead` = [`DEFAULT_LOOKAHEAD`]
    /// * `use_spk_cache` = `false`
    pub fn new(network: Network) -> Self {
        Self {
            secp: SecpCtx::new(),
            descriptors: BTreeMap::new(),
            network,
            genesis_hash: None,
            lookahead: DEFAULT_LOOKAHEAD,
            use_spk_cache: false,
            error: None,
        }
    }

    /// Assign `descriptor` to `keychain`.
    ///
    /// The descriptor is validated immediately, but any error is only reported when the wallet is
    /// created. Creation fails with a [`CreateError`] if `descriptor` cannot be parsed, does not
    /// match the network, fails the checks applied to every wallet descriptor (multipath, hardened
    /// derivation in a public descriptor, miniscript sanity), if `keychain` was already added, or
    /// if `descriptor` is already assigned to another keychain.
    ///
    /// Two descriptors count as the same descriptor when they derive the same script pubkeys — the
    /// same notion of identity the wallet's indexer uses, see [`DescriptorId`]. Descriptors that
    /// are written differently but derive the same scripts, such as the same xpub with and without
    /// origin information, are therefore rejected.
    ///
    /// Once an error is recorded, further calls are ignored.
    ///
    /// [`DescriptorId`]: bdk_chain::DescriptorId
    pub fn descriptor<D: IntoWalletDescriptor>(mut self, keychain: K, descriptor: D) -> Self {
        if self.error.is_none() {
            if let Err(err) = self.add_descriptor(keychain, descriptor) {
                self.error = Some(err);
            }
        }
        self
    }

    fn add_descriptor<D: IntoWalletDescriptor>(
        &mut self,
        keychain: K,
        descriptor: D,
    ) -> Result<(), CreateError<K>> {
        let (descriptor, _keymap) =
            descriptor.into_wallet_descriptor(&self.secp, self.network.into())?;
        check_wallet_descriptor(&descriptor)?;

        if self.descriptors.contains_key(&keychain) {
            return Err(CreateError::KeychainAlreadyAssigned(keychain));
        }
        let descriptor_id = descriptor.descriptor_id();
        if let Some((existing_keychain, _)) = self
            .descriptors
            .iter()
            .find(|(_, d)| d.descriptor_id() == descriptor_id)
        {
            return Err(CreateError::DescriptorAlreadyAssigned {
                keychain,
                existing_keychain: existing_keychain.clone(),
                descriptor: Box::new(descriptor),
            });
        }

        self.descriptors.insert(keychain, descriptor);
        Ok(())
    }

    /// Use a custom `genesis_hash`.
    pub fn genesis_hash(mut self, genesis_hash: BlockHash) -> Self {
        self.genesis_hash = Some(genesis_hash);
        self
    }

    /// Use a custom `lookahead` value.
    ///
    /// The `lookahead` defines a number of script pubkeys to derive over and above the last
    /// revealed index. Without a lookahead the indexer will miss outputs you own when processing
    /// transactions whose output script pubkeys lie beyond the last revealed index. In most cases
    /// the default value [`DEFAULT_LOOKAHEAD`] is sufficient.
    pub fn lookahead(mut self, lookahead: u32) -> Self {
        self.lookahead = lookahead;
        self
    }

    /// Use a persistent cache of indexed script pubkeys (SPKs).
    ///
    /// **Note:** To persist across restarts, this option must also be set at load time with
    /// [`LoadParams`](LoadParams::use_spk_cache).
    pub fn use_spk_cache(mut self, use_spk_cache: bool) -> Self {
        self.use_spk_cache = use_spk_cache;
        self
    }

    /// Create [`PersistedWallet`] with the given [`WalletPersister`].
    pub fn create_wallet<P>(
        self,
        persister: &mut P,
    ) -> Result<PersistedWallet<K, P>, CreateWithPersistError<P::Error, K>>
    where
        P: WalletPersister<K>,
    {
        PersistedWallet::create(persister, self)
    }

    /// Create [`PersistedWallet`] with the given [`AsyncWalletPersister`].
    pub async fn create_wallet_async<P>(
        self,
        persister: &mut P,
    ) -> Result<PersistedWallet<K, P>, CreateWithPersistError<P::Error, K>>
    where
        P: AsyncWalletPersister<K>,
    {
        PersistedWallet::create_async(persister, self).await
    }

    /// Create a [`Wallet`] without persistence.
    pub fn create_wallet_no_persist(self) -> Result<Wallet<K>, CreateError<K>> {
        Wallet::create_with_params(self)
    }
}

/// Parameters for [`Wallet::load`] or [`PersistedWallet::load`].
#[must_use]
pub struct LoadParams<K: Ord> {
    pub(crate) lookahead: u32,
    pub(crate) check_network: Option<Network>,
    pub(crate) check_genesis_hash: Option<BlockHash>,
    pub(crate) check_descriptors: BTreeMap<K, Option<DescriptorToExtract>>,
    pub(crate) use_spk_cache: bool,
}

impl<K: Ord + Clone + core::fmt::Debug> LoadParams<K> {
    /// Construct parameters with default values.
    ///
    /// Default values: `lookahead` = [`DEFAULT_LOOKAHEAD`]
    pub fn new() -> Self {
        Self {
            lookahead: DEFAULT_LOOKAHEAD,
            check_network: None,
            check_genesis_hash: None,
            check_descriptors: BTreeMap::new(),
            use_spk_cache: false,
        }
    }

    /// Checks the `expected_descriptor` matches exactly what is loaded for `keychain`.
    pub fn descriptor<D>(mut self, keychain: K, expected_descriptor: Option<D>) -> Self
    where
        D: IntoWalletDescriptor + Send + 'static,
    {
        let expected = expected_descriptor.map(|d| make_descriptor_to_extract(d));
        self.check_descriptors.insert(keychain, expected);
        self
    }

    /// Checks that the given network matches the one loaded from persistence.
    pub fn check_network(mut self, network: Network) -> Self {
        self.check_network = Some(network);
        self
    }

    /// Checks that the given `genesis_hash` matches the one loaded from persistence.
    pub fn check_genesis_hash(mut self, genesis_hash: BlockHash) -> Self {
        self.check_genesis_hash = Some(genesis_hash);
        self
    }

    /// Use a custom `lookahead` value.
    ///
    /// The `lookahead` defines a number of script pubkeys to derive over and above the last
    /// revealed index. Without a lookahead the indexer will miss outputs you own when processing
    /// transactions whose output script pubkeys lie beyond the last revealed index. In most cases
    /// the default value [`DEFAULT_LOOKAHEAD`] is sufficient.
    pub fn lookahead(mut self, lookahead: u32) -> Self {
        self.lookahead = lookahead;
        self
    }

    /// Use a persistent cache of indexed script pubkeys (SPKs).
    ///
    /// NOTE: This should only be used if you have previously persisted a cache of script
    /// pubkeys using [`CreateParams::use_spk_cache`].
    pub fn use_spk_cache(mut self, use_spk_cache: bool) -> Self {
        self.use_spk_cache = use_spk_cache;
        self
    }

    /// Load [`PersistedWallet`] with the given [`WalletPersister`].
    #[allow(clippy::type_complexity)]
    pub fn load_wallet<P>(
        self,
        persister: &mut P,
    ) -> Result<Option<PersistedWallet<K, P>>, LoadWithPersistError<P::Error, K>>
    where
        P: WalletPersister<K>,
    {
        PersistedWallet::load(persister, self)
    }

    /// Load [`PersistedWallet`] with the given [`AsyncWalletPersister`].
    #[allow(clippy::type_complexity)]
    pub async fn load_wallet_async<P>(
        self,
        persister: &mut P,
    ) -> Result<Option<PersistedWallet<K, P>>, LoadWithPersistError<P::Error, K>>
    where
        P: AsyncWalletPersister<K>,
    {
        PersistedWallet::load_async(persister, self).await
    }

    /// Load [`Wallet`] without persistence.
    pub fn load_wallet_no_persist(
        self,
        changeset: ChangeSet<K>,
    ) -> Result<Option<Wallet<K>>, LoadError<K>> {
        Wallet::load_with_params(changeset, self)
    }
}

impl<K: Ord + Clone + core::fmt::Debug> Default for LoadParams<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod test {
    use super::*;
    use crate::KeychainKind;
    use alloc::vec;
    use alloc::vec::Vec;
    use bdk_chain::DescriptorExt;

    const EXTERNAL: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
    const INTERNAL: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";

    #[test]
    fn creates_with_one_keychain() {
        let wallet = CreateParams::new(Network::Testnet)
            .descriptor(KeychainKind::External, EXTERNAL)
            .create_wallet_no_persist()
            .unwrap();
        assert_eq!(wallet.network(), Network::Testnet);
        assert_eq!(wallet.keychains().count(), 1);
    }

    #[test]
    fn rejects_no_keychains() {
        assert!(matches!(
            CreateParams::<KeychainKind>::new(Network::Testnet).create_wallet_no_persist(),
            Err(CreateError::MissingDescriptors)
        ));
    }

    #[test]
    fn rejects_duplicate_keychain() {
        assert!(matches!(
            CreateParams::new(Network::Testnet)
                .descriptor(KeychainKind::External, EXTERNAL)
                .descriptor(KeychainKind::External, INTERNAL)
                .create_wallet_no_persist(),
            Err(CreateError::KeychainAlreadyAssigned(KeychainKind::External))
        ));
    }

    #[test]
    fn rejects_duplicate_descriptor() {
        assert!(matches!(
            CreateParams::new(Network::Testnet)
                .descriptor(KeychainKind::External, EXTERNAL)
                .descriptor(KeychainKind::Internal, EXTERNAL)
                .create_wallet_no_persist(),
            Err(CreateError::DescriptorAlreadyAssigned {
                keychain: KeychainKind::Internal,
                existing_keychain: KeychainKind::External,
                ..
            })
        ));
    }

    /// The same xpub written with and without origin information derives identical scripts, but
    /// the two descriptors are not structurally equal. Comparing with `==` would let this pass,
    /// and the indexer — which keys on the derived script — would then reject it from inside an
    /// `expect`.
    #[test]
    fn rejects_equivalent_descriptor_written_differently() {
        const WITH_ORIGIN: &str = "wpkh([e273fe42/84'/1'/0']tpubDCmr3Luq75npLaYmRqqW1rLfSbfpnBXwLwAmUbR333fp95wjCHar3zoc9zSWovZFwrWr53mm3NTVqt6d1Pt6G26uf4etQjc3Pr5Hxe9QEQ2/0/*)";
        const NO_ORIGIN: &str = "wpkh(tpubDCmr3Luq75npLaYmRqqW1rLfSbfpnBXwLwAmUbR333fp95wjCHar3zoc9zSWovZFwrWr53mm3NTVqt6d1Pt6G26uf4etQjc3Pr5Hxe9QEQ2/0/*)";

        let params = CreateParams::new(Network::Testnet)
            .descriptor(KeychainKind::External, WITH_ORIGIN)
            .descriptor(KeychainKind::Internal, NO_ORIGIN);
        let existing = params.descriptors[&KeychainKind::External].clone();
        let rejected = match params.create_wallet_no_persist() {
            Err(CreateError::DescriptorAlreadyAssigned { descriptor, .. }) => *descriptor,
            other => panic!("expected DescriptorAlreadyAssigned, got {other:?}"),
        };

        // The premise of the test: structurally distinct, but the same keychain to the indexer.
        assert_ne!(existing, rejected);
        assert_eq!(existing.descriptor_id(), rejected.descriptor_id());
    }

    #[test]
    fn rejects_wrong_network() {
        // A testnet xpriv in a wallet created for Bitcoin.
        assert!(matches!(
            CreateParams::new(Network::Bitcoin)
                .descriptor(KeychainKind::External, EXTERNAL)
                .create_wallet_no_persist(),
            Err(CreateError::Descriptor(_))
        ));
    }

    #[test]
    fn rejects_multipath_descriptor() {
        const MULTIPATH: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/<0;1>/*)";
        assert!(matches!(
            CreateParams::new(Network::Testnet)
                .descriptor(KeychainKind::External, MULTIPATH)
                .create_wallet_no_persist(),
            Err(CreateError::Descriptor(_))
        ));
    }

    #[test]
    fn first_error_is_kept() {
        // The duplicate keychain is the first error; the later invalid descriptor is ignored.
        assert!(matches!(
            CreateParams::new(Network::Testnet)
                .descriptor(KeychainKind::External, EXTERNAL)
                .descriptor(KeychainKind::External, INTERNAL)
                .descriptor(KeychainKind::Internal, "not a descriptor")
                .create_wallet_no_persist(),
            Err(CreateError::KeychainAlreadyAssigned(KeychainKind::External))
        ));
    }

    #[test]
    fn keychains_iterate_in_ord_order() {
        let wallet = CreateParams::new(Network::Testnet)
            .descriptor(KeychainKind::Internal, INTERNAL)
            .descriptor(KeychainKind::External, EXTERNAL)
            .create_wallet_no_persist()
            .unwrap();
        // External < Internal by declaration order, regardless of insertion order.
        let order: Vec<_> = wallet.keychains().map(|(k, _)| k).collect();
        assert_eq!(order, vec![KeychainKind::External, KeychainKind::Internal]);
    }
}

#![allow(unused)]
use alloc::{boxed::Box, collections::btree_map::BTreeMap, fmt};
use bdk_chain::keychain_txout::DEFAULT_LOOKAHEAD;
use bitcoin::{BlockHash, Network, NetworkKind};
use miniscript::descriptor::KeyMap;

use crate::{
    descriptor::{DescriptorError, ExtendedDescriptor, IntoWalletDescriptor},
    keyring::{self, KeyRing, KeyRingError},
    utils::SecpCtx, // WalletPersister,
    AsyncWalletPersister,
    CreateWithPersistError,
    KeychainKind,
    LoadWithPersistError,
    PersistedWallet,
    Wallet,
    WalletPersister,
};

use super::{
    ChangeSet,
    LoadError,
    // PersistedWallet
};

fn make_two_path_descriptor_to_extract<D>(
    two_path_descriptor: D,
    index: usize,
) -> DescriptorToExtract
where
    D: IntoWalletDescriptor + Send + 'static,
{
    Box::new(move |secp, network| {
        let (desc, keymap) = two_path_descriptor.into_wallet_descriptor(secp, network)?;

        if !desc.is_multipath() {
            return Err(DescriptorError::MultiPath);
        }

        let descriptors = desc
            .into_single_descriptors()
            .map_err(DescriptorError::Miniscript)?;

        if descriptors.len() != 2 {
            return Err(DescriptorError::MultiPath);
        }

        Ok((descriptors[index].clone(), keymap))
    })
}

/// This atrocity is to avoid having type parameters on [`CreateParams`] and [`LoadParams`].
///
/// The better option would be to do `Box<dyn IntoWalletDescriptor>`, but we cannot due to Rust's
/// [object safety rules](https://doc.rust-lang.org/reference/items/traits.html#object-safety).
pub(crate) type DescriptorToExtract = Box<
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
#[must_use]
pub struct CreateParams<K> {
    pub(crate) keyring: KeyRing<K>,
    pub(crate) genesis_hash: Option<BlockHash>,
    pub(crate) lookahead: u32,
    pub(crate) use_spk_cache: bool,
}

impl<K> CreateParams<K>
where
    K: Ord + Clone + fmt::Debug,
{
    /// Construct parameters with provided `keyring`.
    ///
    /// Default values:
    /// * `genesis_hash` = `None`
    /// * `lookahead` = [`DEFAULT_LOOKAHEAD`]
    /// * `use_spk_cache` = `false`
    pub fn new(keyring: KeyRing<K>) -> Self {
        Self {
            keyring,
            genesis_hash: None,
            lookahead: DEFAULT_LOOKAHEAD,
            use_spk_cache: false,
        }
    }

    // /// Construct parameters with a two-path descriptor that will be parsed into receive and
    // change /// descriptors.
    // ///
    // /// This function parses a two-path descriptor (receive and change) and creates parameters
    // /// using the existing receive and change wallet creation logic.
    // ///
    // /// Default values:
    // /// * `network` = [`Network::Bitcoin`]
    // /// * `genesis_hash` = `None`
    // /// * `lookahead` = [`DEFAULT_LOOKAHEAD`]
    // pub fn new_two_path<D: IntoWalletDescriptor + Send + Clone + 'static>(
    //     two_path_descriptor: D,
    // ) -> Self {
    //     Self {
    //         descriptor: make_two_path_descriptor_to_extract(two_path_descriptor.clone(), 0),
    //         descriptor_keymap: KeyMap::default(),
    //         change_descriptor: Some(make_two_path_descriptor_to_extract(two_path_descriptor, 1)),
    //         change_descriptor_keymap: KeyMap::default(),
    //         network: Network::Bitcoin,
    //         genesis_hash: None,
    //         lookahead: DEFAULT_LOOKAHEAD,
    //         use_spk_cache: false,
    //     }
    // }

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
    ) -> Result<PersistedWallet<P, K>, CreateWithPersistError<P::Error, K>>
    where
        P: WalletPersister<K>,
    {
        PersistedWallet::create(persister, self)
    }

    /// Create [`PersistedWallet`] with the given [`AsyncWalletPersister`].
    pub async fn create_wallet_async<P>(
        self,
        persister: &mut P,
    ) -> Result<PersistedWallet<P, K>, CreateWithPersistError<P::Error, K>>
    where
        P: AsyncWalletPersister<K>,
    {
        PersistedWallet::create_async(persister, self).await
    }

    /// Create [`Wallet`] without persistence.
    pub fn create_wallet_no_persist(self) -> Result<Wallet<K>, KeyRingError<K>> {
        Wallet::create_with_params(self)
    }
}

type PersistedWalletOption<P, K> = Option<PersistedWallet<P, K>>;

/// Parameters for [`Wallet::load`] or [`PersistedWallet::load`].
#[must_use]
pub struct LoadParams<K> {
    pub(crate) lookahead: u32,
    pub(crate) check_network: Option<Network>,
    pub(crate) check_genesis_hash: Option<BlockHash>,
    pub(crate) check_descs: BTreeMap<K, Option<DescriptorToExtract>>, /* none means just check
                                                                       * keychain is there */
    pub(crate) use_spk_cache: bool,
}

impl<K> LoadParams<K>
where
    K: Ord + Clone + fmt::Debug,
{
    /// Construct parameters with default values.
    ///
    /// Default values: `lookahead` = [`DEFAULT_LOOKAHEAD`]
    pub fn new() -> Self {
        Self {
            lookahead: DEFAULT_LOOKAHEAD,
            check_network: None,
            check_genesis_hash: None,
            check_descs: BTreeMap::new(),
            use_spk_cache: false,
        }
    }

    /// Checks the `expected_descriptor` matches exactly what is loaded for `keychain`.
    pub fn check_descriptor<D>(mut self, keychain: K, expected_descriptor: Option<D>) -> Self
    where
        D: IntoWalletDescriptor + Send + 'static,
        K: Ord,
    {
        let expected = expected_descriptor.map(|d| make_descriptor_to_extract(d));
        self.check_descs.insert(keychain, expected);
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
    pub fn load_wallet<P>(
        self,
        persister: &mut P,
    ) -> Result<PersistedWalletOption<P, K>, LoadWithPersistError<P::Error, K>>
    where
        P: WalletPersister<K>,
    {
        PersistedWallet::load(persister, self)
    }

    /// Load [`PersistedWallet`] with the given [`AsyncWalletPersister`].
    pub async fn load_wallet_async<P>(
        self,
        persister: &mut P,
    ) -> Result<Option<PersistedWallet<P, K>>, LoadWithPersistError<P::Error, K>>
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

impl<K> Default for LoadParams<K>
where
    K: Ord + Clone + fmt::Debug,
{
    fn default() -> Self {
        Self::new()
    }
}

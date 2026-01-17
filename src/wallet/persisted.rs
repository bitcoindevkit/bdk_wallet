#![allow(unused)]
use core::{
    fmt,
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
};

use alloc::{boxed::Box, string::ToString};
use chain::Merge;

use bitcoin::BlockHash;

use crate::{
    descriptor::{calc_checksum, DescriptorError},
    keyring, ChangeSet, CreateParams, LoadParams, Wallet,
};

use crate::keyring::KeyRing;

/// Trait that persists [`PersistedWallet`].
///
/// For an async version, use [`AsyncWalletPersister`].
///
/// Associated functions of this trait should not be called directly, and the trait is designed so
/// that associated functions are hard to find (since they are not methods!). [`WalletPersister`] is
/// used by [`PersistedWallet`] (a light wrapper around [`Wallet`]) which enforces some level of
/// safety. Refer to [`PersistedWallet`] for more about the safety checks.
pub trait WalletPersister<K>
where
    K: Ord,
{
    /// Error type of the persister.
    type Error;

    /// Initialize the `persister` and load all data.
    ///
    /// This is called by [`PersistedWallet::create`] and [`PersistedWallet::load`] to ensure
    /// the [`WalletPersister`] is initialized and returns all data in the `persister`.
    ///
    /// # Implementation Details
    ///
    /// The database schema of the `persister` (if any), should be initialized and migrated here.
    ///
    /// The implementation must return all data currently stored in the `persister`. If there is no
    /// data, return an empty changeset (using [`ChangeSet::default()`]).
    ///
    /// Error should only occur on database failure. Multiple calls to `initialize` should not
    /// error. Calling `initialize` in between calls to `persist` should not error.
    ///
    /// Calling [`persist`] before the `persister` is `initialize`d may error. However, some
    /// persister implementations may NOT require initialization at all (and not error).
    ///
    /// [`persist`]: WalletPersister::persist
    fn initialize(persister: &mut Self) -> Result<ChangeSet<K>, Self::Error>;

    /// Persist the given `changeset` to the `persister`.
    ///
    /// This method can fail if the `persister` is not [`initialize`]d.
    ///
    /// [`initialize`]: WalletPersister::initialize
    fn persist(persister: &mut Self, changeset: &ChangeSet<K>) -> Result<(), Self::Error>;
}

type FutureResult<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Async trait that persists [`PersistedWallet`].
///
/// For a blocking version, use [`WalletPersister`].
///
/// Associated functions of this trait should not be called directly, and the trait is designed so
/// that associated functions are hard to find (since they are not methods!).
/// [`AsyncWalletPersister`] is used by [`PersistedWallet`] (a light wrapper around [`Wallet`])
/// which enforces some level of safety. Refer to [`PersistedWallet`] for more about the safety
/// checks.
pub trait AsyncWalletPersister<K>
where
    K: Ord,
{
    /// Error type of the persister.
    type Error;

    /// Initialize the `persister` and load all data.
    ///
    /// This is called by [`PersistedWallet::create_async`] and [`PersistedWallet::load_async`] to
    /// ensure the [`AsyncWalletPersister`] is initialized and returns all data in the `persister`.
    ///
    /// # Implementation Details
    ///
    /// The database schema of the `persister` (if any), should be initialized and migrated here.
    ///
    /// The implementation must return all data currently stored in the `persister`. If there is no
    /// data, return an empty changeset (using [`ChangeSet::default()`]).
    ///
    /// Error should only occur on database failure. Multiple calls to `initialize` should not
    /// error. Calling `initialize` in between calls to `persist` should not error.
    ///
    /// Calling [`persist`] before the `persister` is `initialize`d may error. However, some
    /// persister implementations may NOT require initialization at all (and not error).
    ///
    /// [`persist`]: AsyncWalletPersister::persist
    fn initialize<'a>(persister: &'a mut Self) -> FutureResult<'a, ChangeSet<K>, Self::Error>
    where
        Self: 'a;

    /// Persist the given `changeset` to the `persister`.
    ///
    /// This method can fail if the `persister` is not [`initialize`]d.
    ///
    /// [`initialize`]: AsyncWalletPersister::initialize
    fn persist<'a>(
        persister: &'a mut Self,
        changeset: &'a ChangeSet<K>,
    ) -> FutureResult<'a, (), Self::Error>
    where
        Self: 'a;
}

/// Represents a persisted wallet which persists into type `P`.
///
/// This is a light wrapper around [`Wallet`] that enforces some level of safety-checking when used
/// with a [`WalletPersister`] or [`AsyncWalletPersister`] implementation. Safety checks assume that
/// [`WalletPersister`] and/or [`AsyncWalletPersister`] are implemented correctly.
///
/// Checks include:
///
/// * Ensure the persister is initialized before data is persisted.
/// * Ensure there were no previously persisted wallet data before creating a fresh wallet and
///   persisting it.
/// * Only clear the staged changes of [`Wallet`] after persisting succeeds.
/// * Ensure the wallet is persisted to the same `P` type as when created/loaded. Note that this is
///   not completely fool-proof as you can have multiple instances of the same `P` type that are
///   connected to different databases.
#[derive(Debug)]
pub struct PersistedWallet<P, K>
where
    K: Ord,
{
    inner: Wallet<K>,
    _marker: PhantomData<fn(&mut P)>,
}

impl<P, K> Deref for PersistedWallet<P, K>
where
    K: Ord,
{
    type Target = Wallet<K>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<P, K> DerefMut for PersistedWallet<P, K>
where
    K: Ord,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Methods when `P` is a [`WalletPersister`].
impl<P, K> PersistedWallet<P, K>
where
    K: Ord + Clone + fmt::Debug,
    P: WalletPersister<K>,
{
    /// Create a new [`PersistedWallet`] with the given `persister` and parameters.
    pub fn create(
        persister: &mut P,
        params: CreateParams<K>,
    ) -> Result<Self, CreateWithPersistError<P::Error, K>> {
        let existing = P::initialize(persister).map_err(CreateWithPersistError::Persist)?;
        if !existing.is_empty() {
            return Err(CreateWithPersistError::DataAlreadyExists(Box::new(
                existing,
            )));
        }
        let mut inner = Wallet::create_with_params(params);
        if let Some(changeset) = inner.take_staged() {
            P::persist(persister, &changeset).map_err(CreateWithPersistError::Persist)?;
        }
        Ok(Self {
            inner,
            _marker: PhantomData,
        })
    }

    /// Load a previously [`PersistedWallet`] from the given `persister` and `params`.
    pub fn load(
        persister: &mut P,
        params: LoadParams<K>,
    ) -> Result<Option<Self>, LoadWithPersistError<P::Error, K>> {
        let changeset = P::initialize(persister).map_err(LoadWithPersistError::Persist)?;
        Wallet::from_changeset(changeset, params)
            .map(|opt| {
                opt.map(|inner| PersistedWallet {
                    inner,
                    _marker: PhantomData,
                })
            })
            .map_err(LoadWithPersistError::InvalidChangeSet)
    }

    /// Persist staged changes of wallet into `persister`.
    ///
    /// Returns whether any new changes were persisted.
    ///
    /// If the `persister` errors, the staged changes will not be cleared.
    pub fn persist(&mut self, persister: &mut P) -> Result<bool, P::Error> {
        match self.inner.staged_mut() {
            Some(stage) => {
                P::persist(persister, &*stage)?;
                let _ = stage.take();
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

/// Methods when `P` is an [`AsyncWalletPersister`].
impl<P, K> PersistedWallet<P, K>
where
    K: Ord + Clone + fmt::Debug,
    P: AsyncWalletPersister<K>,
{
    /// Create a new [`PersistedWallet`] with the given async `persister` and `params`.
    pub async fn create_async(
        persister: &mut P,
        params: CreateParams<K>,
    ) -> Result<Self, CreateWithPersistError<P::Error, K>> {
        let existing = P::initialize(persister)
            .await
            .map_err(CreateWithPersistError::Persist)?;
        if !existing.is_empty() {
            return Err(CreateWithPersistError::DataAlreadyExists(Box::new(
                existing,
            )));
        }
        let mut inner = Wallet::create_with_params(params);
        if let Some(changeset) = inner.take_staged() {
            P::persist(persister, &changeset)
                .await
                .map_err(CreateWithPersistError::Persist)?;
        }
        Ok(Self {
            inner,
            _marker: PhantomData,
        })
    }

    /// Load a previously [`PersistedWallet`] from the given async `persister` and `params`.
    pub async fn load_async(
        persister: &mut P,
        params: LoadParams<K>,
    ) -> Result<Option<Self>, LoadWithPersistError<P::Error, K>> {
        let changeset = P::initialize(persister)
            .await
            .map_err(LoadWithPersistError::Persist)?;
        Wallet::from_changeset(changeset, params)
            .map(|opt| {
                opt.map(|inner| PersistedWallet {
                    inner,
                    _marker: PhantomData,
                })
            })
            .map_err(LoadWithPersistError::InvalidChangeSet)
    }

    /// Persist staged changes of wallet into an async `persister`.
    ///
    /// Returns whether any new changes were persisted.
    ///
    /// If the `persister` errors, the staged changes will not be cleared.
    pub async fn persist_async(&mut self, persister: &mut P) -> Result<bool, P::Error> {
        match self.inner.staged_mut() {
            Some(stage) => {
                P::persist(persister, &*stage).await?;
                let _ = stage.take();
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[cfg(feature = "rusqlite")]
use crate::wallet::{FromSql, ToSql};

#[cfg(feature = "rusqlite")]
impl<K: Ord + Clone + FromSql + ToSql> WalletPersister<K> for bdk_chain::rusqlite::Transaction<'_> {
    type Error = bdk_chain::rusqlite::Error;

    fn initialize(persister: &mut Self) -> Result<ChangeSet<K>, Self::Error> {
        ChangeSet::<K>::init_sqlite_tables(&*persister)?;
        ChangeSet::<K>::from_sqlite(persister)
    }

    fn persist(persister: &mut Self, changeset: &ChangeSet<K>) -> Result<(), Self::Error> {
        changeset.persist_to_sqlite(persister)
    }
}

#[cfg(feature = "rusqlite")]
impl<K: Ord + Clone + FromSql + ToSql> WalletPersister<K> for bdk_chain::rusqlite::Connection {
    type Error = bdk_chain::rusqlite::Error;

    fn initialize(persister: &mut Self) -> Result<ChangeSet<K>, Self::Error> {
        let db_tx = persister.transaction()?;
        ChangeSet::<K>::init_sqlite_tables(&db_tx)?;
        let changeset = ChangeSet::<K>::from_sqlite(&db_tx)?;
        db_tx.commit()?;
        Ok(changeset)
    }

    fn persist(persister: &mut Self, changeset: &ChangeSet<K>) -> Result<(), Self::Error> {
        let db_tx = persister.transaction()?;
        changeset.persist_to_sqlite(&db_tx)?;
        db_tx.commit()
    }
}

/// Error for [`bdk_file_store`]'s implementation of [`WalletPersister`].
#[cfg(feature = "file_store")]
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum FileStoreError<K: Ord> {
    /// Error when loading from the store.
    Load(bdk_file_store::StoreErrorWithDump<ChangeSet<K>>),
    /// Error when writing to the store.
    Write(std::io::Error),
}

#[cfg(feature = "file_store")]
impl<K: Ord> core::fmt::Display for FileStoreError<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use core::fmt::Display;
        match self {
            FileStoreError::Load(e) => Display::fmt(e, f),
            FileStoreError::Write(e) => Display::fmt(e, f),
        }
    }
}

#[cfg(feature = "file_store")]
impl<K: Ord + fmt::Debug> std::error::Error for FileStoreError<K> {}

#[cfg(feature = "file_store")]
impl<K: Ord + serde::de::DeserializeOwned + serde::Serialize> WalletPersister<K>
    for bdk_file_store::Store<ChangeSet<K>>
{
    type Error = FileStoreError<K>;

    fn initialize(persister: &mut Self) -> Result<ChangeSet<K>, Self::Error> {
        persister
            .dump()
            .map(Option::unwrap_or_default)
            .map_err(FileStoreError::Load)
    }

    fn persist(persister: &mut Self, changeset: &ChangeSet<K>) -> Result<(), Self::Error> {
        persister.append(changeset).map_err(FileStoreError::Write)
    }
}

/// Error type for [`PersistedWallet::load`].
#[derive(Debug, PartialEq)]
pub enum LoadWithPersistError<E, K> {
    /// Error from persistence.
    Persist(E),
    /// Occurs when the loaded changeset cannot construct [`Wallet`].
    InvalidChangeSet(crate::LoadError<K>),
}

impl<E: fmt::Display, K: fmt::Display> fmt::Display for LoadWithPersistError<E, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persist(err) => fmt::Display::fmt(err, f),
            Self::InvalidChangeSet(err) => fmt::Display::fmt(err, f),
        }
    }
}

#[cfg(feature = "std")]
impl<E: fmt::Debug + fmt::Display, K: fmt::Debug + fmt::Display> std::error::Error
    for LoadWithPersistError<E, K>
{
}

/// Error type for [`PersistedWallet::create`].
#[derive(Debug)]
pub enum CreateWithPersistError<E, K>
where
    K: Ord,
{
    /// Error from persistence.
    Persist(E),
    /// Persister already has wallet data.
    DataAlreadyExists(Box<ChangeSet<K>>),
    /// Occurs when the loaded changeset cannot construct [`Wallet`].
    Descriptor(DescriptorError),
}

impl<E: fmt::Display, K: fmt::Display + Ord> fmt::Display for CreateWithPersistError<E, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persist(err) => write!(f, "{err}"),
            Self::DataAlreadyExists(changeset) => {
                write!(
                    f,
                    "Cannot create wallet in a persister which already contains data: "
                )?;
                changeset_info(f, changeset)
            }
            Self::Descriptor(err) => {
                write!(f, "{err}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl<E: fmt::Debug + fmt::Display, K: Ord + fmt::Debug + fmt::Display> std::error::Error
    for CreateWithPersistError<E, K>
{
}

/// Helper function to display basic information about a [`ChangeSet`].
fn changeset_info<K: Ord + fmt::Display>(
    f: &mut fmt::Formatter<'_>,
    changeset: &ChangeSet<K>,
) -> fmt::Result {
    let network = changeset
        .keyring
        .network
        .as_ref()
        .map_or("None".to_string(), |n| n.to_string());

    writeln!(f, "  Network: {network}")?;

    for (keychain, descriptor) in &changeset.keyring.descriptors {
        let descriptor_checksum = calc_checksum(&descriptor.to_string()).unwrap();
        writeln!(
            f,
            " Keychain: {keychain}, Descriptor Checksum: {descriptor_checksum}"
        );
    }

    writeln!(
        f,
        "Default Keychain: {}",
        changeset
            .keyring
            .default_keychain
            .as_ref()
            .map_or("None".to_string(), |n| n.to_string())
    );

    let tx_count = changeset.tx_graph.txs.len();
    writeln!(f, "  Transaction Count: {tx_count}")?;

    let anchor_count = changeset.tx_graph.anchors.len();
    writeln!(f, "  Anchor Count: {anchor_count}")?;

    let block_count = if let Some(&count) = changeset.local_chain.blocks.keys().last() {
        count
    } else {
        0
    };
    writeln!(f, "  Block Count: {block_count}")?;

    Ok(())
}

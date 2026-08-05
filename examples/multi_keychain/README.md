# Multi-keychain examples

A `Wallet<K>` tracks any number of keychains, each identified by a value of type `K`. The default
is `KeychainKind`, which gives the familiar two-keychain wallet; supplying your own `K` gives you
as many keychains as you need.

The wallet requires `K: Ord + Clone + Debug` and nothing more, so `K` can carry whatever metadata
your application wants to attach to a keychain. `Ord` is required because keychains are held in a
`BTreeMap`, which also means it decides the order they iterate in.

| Example | Shows |
| --- | --- |
| [`custom_keychain.rs`](./custom_keychain.rs) | A keychain identifier that carries application metadata, and using it to choose which keychain to reveal from |
| [`address_generation.rs`](./address_generation.rs) | Per-keychain derivation indices, `reveal_next_address` vs `next_unused_address`, reading back the last revealed index |
| [`persistence.rs`](./persistence.rs) | Persisting a multi-keychain wallet to sqlite and loading it back, including load-time descriptor checks |

Run them with:

```shell
cargo run --example multi_keychain_custom_keychain
cargo run --example multi_keychain_address_generation
cargo run --example multi_keychain_persistence --features rusqlite
```

## Building a wallet

Descriptors go into a `KeyRing`, which validates each one against the network and rejects duplicate
keychains and duplicate descriptors as they are added:

```rust,ignore
let mut keyring = KeyRing::new(Network::Signet, Keychain::A, DESC_A)?;
keyring.add_descriptor(Keychain::B, DESC_B)?;

let mut wallet = Wallet::create(keyring).create_wallet_no_persist();
```

Because the `KeyRing` has already done that checking, building the wallet from it cannot fail —
`create_wallet_no_persist` returns a `Wallet<K>` rather than a `Result`. Descriptor errors surface
at `KeyRing::new` and `KeyRing::add_descriptor`, where the descriptor was actually supplied.

The conventional two-keychain wallet is the same two calls with `KeychainKind`:

```rust,ignore
let mut keyring = KeyRing::new(Network::Signet, KeychainKind::External, external_desc)?;
keyring.add_descriptor(KeychainKind::Internal, internal_desc)?;
```

## Building transactions

`Wallet::create_psbt` and `Wallet::replace_by_fee` work on any `Wallet<K>`. Because a wallet
generic over `K` has no canonical change keychain, you name one explicitly:

```rust,ignore
let mut params = PsbtParams::default();
params
    .add_recipients([(recipient_spk, Amount::from_sat(10_000))])
    .change_keychain(Keychain::Change);

let (psbt, finalizer) = wallet.create_psbt(params)?;
```

Set `change_keychain` or `change_script` — without one, PSBT creation fails with
`CreatePsbtError::NoChangeSource`, and naming a keychain the wallet does not hold fails with
`CreatePsbtError::UnknownChangeKeychain`. Change derived from `change_keychain` is revealed and
staged, so it stays tracked; you must persist the resulting changeset.

## Current limitations

- **No `TxBuilder`.** The older `TxBuilder` API still lives on `impl Wallet<KeychainKind>`, since it
  has no way to be told which keychain change belongs to. Use `create_psbt` instead.
- **Nothing is trusted before it is mined.** `balance()` cannot know which of your keychains hold
  self-owned change, so all unconfirmed output counts as untrusted-pending.

To persist a custom keychain type, implement `rusqlite`'s `ToSql` and `FromSql` for it (see
[`persistence.rs`](./persistence.rs)); for the file store, implement `serde::Serialize` and
`serde::Deserialize`.

<div align="center">
  <h1>BDK</h1>

  <img src="https://raw.githubusercontent.com/bitcoindevkit/bdk/master/static/bdk.png" width="220" />

  <p>
    <strong>A modern, lightweight, descriptor-based wallet library written in Rust!</strong>
  </p>

  <p>
    <a href="https://crates.io/crates/bdk_wallet"><img alt="Crate Info" src="https://img.shields.io/crates/v/bdk_wallet.svg"/></a>
    <a href="https://github.com/bitcoindevkit/bdk/blob/master/LICENSE"><img alt="MIT or Apache-2.0 Licensed" src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg"/></a>
    <a href="https://github.com/bitcoindevkit/bdk/actions?query=workflow%3ACI"><img alt="CI Status" src="https://github.com/bitcoindevkit/bdk/workflows/CI/badge.svg"></a>
    <a href="https://codecov.io/github/bitcoindevkit/bdk_wallet" ><img src="https://codecov.io/github/bitcoindevkit/bdk_wallet/graph/badge.svg"/></a>
    <a href="https://docs.rs/bdk_wallet"><img alt="API Docs" src="https://img.shields.io/badge/docs.rs-bdk_wallet-green"/></a>
    <a href="https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/"><img alt="Rustc Version 1.85.0+" src="https://img.shields.io/badge/rustc-1.85.0%2B-lightgrey.svg"/></a>
    <a href="https://discord.gg/d7NkDKm"><img alt="Chat on Discord" src="https://img.shields.io/discord/753336465005608961?logo=discord"></a>
  </p>

  <h4>
    <a href="https://bitcoindevkit.org">Project Homepage</a>
    <span> | </span>
    <a href="https://docs.rs/bdk_wallet">Documentation</a>
  </h4>
</div>

## About

The `bdk_wallet` project provides a high level descriptor based wallet API for building Bitcoin applications.

## Architecture

[`bdk_wallet`] contains the central high level [`Wallet`] type that is built from the other low-level components.

Core BDK crates that `bdk_wallet` depends on are found in the [`bdk`] repository. This works by
leveraging the functionality in [`rust-bitcoin`] and [`rust-miniscript`].

# BDK Wallet

The `bdk_wallet` provides the [`Wallet`] type which is a simple, high-level
interface built from the low-level components of [`bdk_chain`]. `Wallet` is a good starting point
for many simple applications as well as a good demonstration of how to use the other mechanisms to
construct a wallet. It has two keychains (external and internal) that are defined by
[miniscript descriptors][`rust-miniscript`] and uses them to generate addresses. When you give it
chain data it also uses the descriptors to find transaction outputs owned by them. From there, you
can create and sign transactions.

For details about the API of `Wallet` see the [module-level documentation][`Wallet`].

## Blockchain data

In order to get blockchain data for `Wallet` to consume, you should configure a client from
an available chain source. Typically you make a request to the chain source and get a response
that the `Wallet` can use to update its view of the chain.

**Blockchain Data Sources**

* [`bdk_esplora`]: Gets blockchain data from Esplora for updating BDK structures.
* [`bdk_electrum`]: Gets blockchain data from Electrum for updating BDK structures.
* [`bdk_bitcoind_rpc`]: Gets blockchain data from Bitcoin Core for updating BDK structures.

**Examples**

* [`examples/esplora_async`](https://github.com/bitcoindevkit/bdk_wallet/blob/master/examples/esplora_async.rs)
* [`examples/esplora_blocking`](https://github.com/bitcoindevkit/bdk_wallet/blob/master/examples/esplora_blocking.rs)
* [`examples/electrum`](https://github.com/bitcoindevkit/bdk_wallet/blob/master/examples/electrum.rs)
* [`examples/bitcoind_rpc`](https://github.com/bitcoindevkit/bdk_wallet/blob/master/examples/bitcoind_rpc.rs)

## Persistence

To persist `Wallet` state use a data storage crate that reads and writes [`ChangeSet`].

**Implementations**

* [`bdk_file_store`]: Stores wallet changes in a simple flat file.
* `rusqlite`: Stores wallet changes in a SQLite database.

<!-- **Example**

```rust,no_run
use bdk_wallet::rusqlite;
use bdk_wallet::{KeychainKind, Wallet};

// Open or create a new SQLite database for wallet data.
let db_path = "my_wallet.sqlite";
let mut conn = rusqlite::Connection::open(db_path)?;

let network = bitcoin::Network::Testnet;
let descriptor = "wpkh(tprv8ZgxMBicQKsPdcAqYBpzAFwU5yxBUo88ggoBqu1qPcHUfSbKK1sKMLmC7EAk438btHQrSdu3jGGQa6PA71nvH5nkDexhLteJqkM4dQmWF9g/84'/1'/0'/0/*)";
let change_descriptor = "wpkh(tprv8ZgxMBicQKsPdcAqYBpzAFwU5yxBUo88ggoBqu1qPcHUfSbKK1sKMLmC7EAk438btHQrSdu3jGGQa6PA71nvH5nkDexhLteJqkM4dQmWF9g/84'/1'/0'/1/*)";

let mut wallet = match Wallet::load()
    .descriptor(KeychainKind::External, Some(descriptor))
    .descriptor(KeychainKind::Internal, Some(change_descriptor))
    .extract_keys()
    .check_network(network)
    .load_wallet(&mut conn)?
{
    Some(wallet) => wallet,
    None => Wallet::create(descriptor, change_descriptor)
        .network(network)
        .create_wallet(&mut conn)?,
};

// Get a new address to receive bitcoin!
let address_info = wallet.reveal_next_address(KeychainKind::External);

// Persist new wallet state to database.
wallet.persist(&mut conn)?;

println!("Next receive address: {}", address_info.address);
Ok::<_, anyhow::Error>(())
``` -->

## Minimum Supported Rust Version (MSRV)

The libraries in this repository maintain a MSRV of 1.85.0.

To build with the MSRV of 1.85.0 you may need to pin dependencies by running the [`pin-msrv.sh`](./ci/pin-msrv.sh) script.

## Just

This project has a [`justfile`](/justfile) for easy command running. You must have [`just`](https://github.com/casey/just) installed.

To see a list of available recipes: `just -l`

## Testing

### Unit testing

```bash
just test
```

# License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

# Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

[`Wallet`]: https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.Wallet.html
[`ChangeSet`]: https://docs.rs/bdk_wallet/latest/bdk_wallet/struct.ChangeSet.html
[`bdk`]: https://github.com/bitcoindevkit/bdk
[`bdk_wallet`]: https://docs.rs/bdk_wallet/latest
[`bdk_chain`]: https://docs.rs/bdk_chain/latest
[`bdk_file_store`]: https://docs.rs/bdk_file_store/latest
[`bdk_electrum`]: https://docs.rs/bdk_electrum/latest
[`bdk_esplora`]: https://docs.rs/bdk_esplora/latest
[`bdk_bitcoind_rpc`]: https://docs.rs/bdk_bitcoind_rpc/latest
[`rust-bitcoin`]: https://docs.rs/bitcoin/latest/bitcoin/
[`rust-miniscript`]: https://docs.rs/miniscript/latest/miniscript/index.html

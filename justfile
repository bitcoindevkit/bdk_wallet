import? "justfile.local"

alias b := build
alias c := check
alias f := fmt
alias t := test
alias p := pre-push
alias d := doc

# All features not including the unstable ones
FEATURES := "std,compiler,all-keys,rusqlite,file_store,test-utils"

_default:
   @just --list

# Build the project
build:
   cargo build

# Check formatting, compilation, linting
_check:
   cargo +nightly-2026-08-01 fmt --all -- --check
   RUSTFLAGS="" cargo check --all-targets --no-default-features --features miniscript/no-std,bdk_chain/hashbrown
   RUSTFLAGS="" cargo check --all-targets --features {{FEATURES}}
   RUSTFLAGS="-D warnings" cargo clippy --all-targets --features {{FEATURES}}

# Check formatting, compilation, linting of the unstable API surface
_check-unstable:
   cargo +nightly-2026-08-01 fmt --all -- --check
   RUSTFLAGS="--cfg bdk_wallet_unstable" cargo check --all-targets --no-default-features --features miniscript/no-std,bdk_chain/hashbrown
   RUSTFLAGS="--cfg bdk_wallet_unstable" cargo check --all-targets --all-features
   RUSTFLAGS="--cfg bdk_wallet_unstable -D warnings" cargo clippy --all-targets --all-features

# Check formatting, compilation, linting, and commit signature
check: _check _check-unstable
   @[ "$(git log --pretty='format:%G?' -1 HEAD)" = "N" ] && \
       echo "\n⚠️  Unsigned commit: BDK requires that commits be signed." || \
       true

# Format all code
fmt:
   cargo +nightly-2026-08-01 fmt

# Run tests on the stable API surface
_test:
   RUSTFLAGS="" cargo test --workspace --features {{FEATURES}}

# Run tests on the unstable API surface (with --cfg bdk_wallet_unstable)
_test-unstable:
   RUSTDOCFLAGS="--cfg bdk_wallet_unstable" RUSTFLAGS="--cfg bdk_wallet_unstable" cargo test --workspace --all-features

# Run all tests on the workspace with all features
test: _test _test-unstable

# Check docs on the workspace
doc:
   RUSTDOCFLAGS="-D warnings --cfg bdk_wallet_unstable" cargo doc --workspace --all-features --no-deps

# Run pre-push suite: format, check, and test
pre-push: fmt check test doc

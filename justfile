import? "justfile.local"

alias b := build
alias c := check
alias f := fmt
alias t := test
alias p := pre-push
alias d := doc
alias l := lock

# secp256k1's C code needs a compiler for RBMT's embedded target.
export CC_thumbv7m_none_eabi := env_var_or_default("CC_thumbv7m_none_eabi", "clang")

# All features not including the unstable ones
FEATURES := "std,compiler,all-keys,rusqlite,file_store,test-utils"

_default:
   @just --list

# Build the project
build:
   cargo rbmt run --toolchain stable --lockfile recent -- build

# Check formatting, compilation, linting
_check:
   cargo rbmt fmt --check
   RUSTFLAGS="" cargo rbmt run --toolchain stable --lockfile recent -- check --all-targets --no-default-features
   RUSTFLAGS="" cargo rbmt run --toolchain stable --lockfile recent -- check --all-targets --features {{FEATURES}}
   RUSTFLAGS="-D warnings" cargo rbmt run --toolchain stable --lockfile recent -- clippy --all-targets --features {{FEATURES}}

# Check compilation and RBMT linting with the unstable API enabled
_check-unstable:
   RUSTFLAGS="--cfg bdk_wallet_unstable" cargo rbmt run --toolchain stable --lockfile recent -- check --all-targets --no-default-features
   RUSTFLAGS="--cfg bdk_wallet_unstable" cargo rbmt run --toolchain stable --lockfile recent -- check --all-targets --all-features
   RUSTFLAGS="--cfg bdk_wallet_unstable" cargo rbmt lint

# Check formatting, compilation, linting, and commit signature
check: _check _check-unstable
   @[ "$(git log --pretty='format:%G?' -1 HEAD)" = "N" ] && \
       echo "\n⚠️  Unsigned commit: BDK requires that commits be signed." || \
       true

# Format all code
fmt:
   cargo rbmt fmt

# Run tests on the stable API surface
_test:
   RUSTFLAGS="" cargo rbmt run --toolchain stable --lockfile recent -- test --workspace --features {{FEATURES}}

# Run tests on the unstable API surface (with --cfg bdk_wallet_unstable)
_test-unstable:
   RUSTDOCFLAGS="--cfg bdk_wallet_unstable" RUSTFLAGS="--cfg bdk_wallet_unstable" cargo rbmt test --toolchain stable --lockfile recent
   RUSTDOCFLAGS="--cfg bdk_wallet_unstable" RUSTFLAGS="--cfg bdk_wallet_unstable" cargo rbmt test --toolchain msrv --lockfile minimal

# Run stable API tests and the stable/recent and MSRV/minimal RBMT matrices
test: _test _test-unstable

# Check docs on the workspace
doc:
   RUSTDOCFLAGS="-D warnings --cfg bdk_wallet_unstable" cargo rbmt run --toolchain stable --lockfile recent -- doc --workspace --all-features --no-deps

# Regenerate the committed dependency lockfiles
lock:
   RUSTFLAGS="--cfg bdk_wallet_unstable" cargo rbmt lock

# Run pre-push suite: regenerate Lockfiles, format, check, test and build docs
pre-push: lock fmt check test doc

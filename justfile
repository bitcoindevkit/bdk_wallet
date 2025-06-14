alias b := build
alias c := check
alias f := fmt
alias t := test
alias p := pre-push

build:
    cargo build

check:
    cargo +nightly fmt --all -- --check
    cargo check --workspace --exclude 'example_*' --all-features
    cargo clippy --all-features --all-targets -- -D warnings
    @[ "$(git log --pretty='format:%G?' -1 HEAD)" = "N" ] && \
        echo "\n⚠️  Unsigned commit: BDK requires that commits be signed." || \
        true

fmt:
    cargo +nightly fmt

test:
    cargo test --workspace --exclude 'example_*' --all-features

pre-push: fmt check test

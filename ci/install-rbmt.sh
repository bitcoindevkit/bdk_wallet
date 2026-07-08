#!/usr/bin/env bash
set -euo pipefail

version=$(cat "$(git rev-parse --show-toplevel)/rbmt-version")
# Strip optional "cargo-rbmt-" or "v" prefix so the file can hold any common version format
version="${version#cargo-rbmt-}"
version="${version#v}"

cargo install cargo-rbmt --version "$version" --locked

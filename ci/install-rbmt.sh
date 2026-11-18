#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
if ! version=$(grep '^rbmt\.version = "' Cargo.toml | cut -d '"' -f 2) || [[ -z "$version" ]]; then
    echo 'Missing RBMT version pin in Cargo.toml' >&2
    exit 1
fi

if [[ "$(cargo rbmt version 2>/dev/null)" == "cargo-rbmt $version" ]]; then
    exit 0
fi

cargo install --locked "cargo-rbmt@$version"

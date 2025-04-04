#!/bin/bash

set -x
set -euo pipefail

# Pin dependencies for MSRV

# To pin deps, switch toolchain to MSRV and execute the below updates

# cargo clean
# rustup override set 1.63.0

cargo update -p home --precise "0.5.5"
cargo update -p url --precise "2.5.0"
cargo update -p tokio --precise "1.38.1"
cargo update -p tokio-util --precise "0.7.11"
cargo update -p indexmap --precise "2.5.0"
cargo update -p security-framework-sys --precise "2.11.1"
cargo update -p ring --precise "0.17.12"
cargo update -p once_cell --precise "1.20.3"
cargo update -p minreq --precise "2.13.2"
cargo update -p native-tls --precise "0.2.13"
#!/bin/bash

set -x
set -euo pipefail

# Pin dependencies for MSRV

# To pin deps, switch toolchain to MSRV and execute the below updates

# cargo clean
# rustup override set 1.63.0

cargo update -p libc --precise "0.2.183"
cargo update -p getrandom@0.4.2 --precise 0.3.4
cargo update -p once_cell --precise "1.20.3"
cargo update -p quote --precise "1.0.41"
cargo update -p serde_json --precise "1.0.145"
cargo update -p anyhow --precise "1.0.100"
cargo update -p tempfile --precise "3.25.0"
cargo update -p proc-macro2 --precise "1.0.103"
cargo update -p ryu --precise "1.0.20"
cargo update -p syn --precise "2.0.106"
cargo update -p itoa --precise "1.0.15"
cargo update -p unicode-ident --precise "1.0.22"

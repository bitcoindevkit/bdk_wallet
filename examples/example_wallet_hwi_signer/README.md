# Example signing with HWI Interface


## Requirements

sudo apt install libudev-dev

## Build and run

`$ cargo run --bin example_wallet_hwi_signer`

## Running with simulator

Download a simulator at `https://github.com/BitBoxSwiss/bitbox02-firmware/releases/`.

Run the simulator and then run the example with `--features=simulator` enabled.

```sh

curl https://github.com/BitBoxSwiss/bitbox02-firmware/releases/download/firmware%2Fv9.19.0/bitbox02-multi-v9.19.0-simulator1.0.0-linux-amd64

./bitbox02-multi-v9.19.0-simulator1.0.0-linux-amd64

cargo run --bin example_wallet_hwi_signer --features simulator
```



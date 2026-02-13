# BDK Wallet Fuzzing

This crate provides structure-aware fuzzing for BDK wallet components using libfuzzer.

## Architecture

The fuzzing infrastructure uses the `Arbitrary` trait for structured input generation instead of raw byte manipulation. This provides:

- **Type Safety**: Compile-time guarantees for input structure
- **Better Coverage**: Smart generation of valid wallet operations
- **Maintainability**: Clear, readable code without macro magic
- **Efficiency**: Focused generation of interesting test cases

## Key Components

- `arbitrary_types.rs`: Defines all fuzzed types with `Arbitrary` implementations
  - Basic types (TxId, BlockHash, Amount, etc.)
  - Wallet operations (ApplyUpdate, CreateTransaction, PersistAndLoad)
  - Transaction building with comprehensive options
  - Complete wallet updates with chain data

## How does it work?

The fuzzer generates structured inputs using the `Arbitrary` trait:

1. `FuzzInput` contains a sequence of `FuzzedWalletOperation`s
2. Each operation is one of: ApplyUpdate, CreateTransaction, or PersistAndLoad
3. Operations contain realistic data with weighted probabilities
4. The fuzzer executes all operations on a fresh wallet instance
5. Errors are handled gracefully to continue fuzzing

## How do I run the fuzz tests locally?

Install cargo-fuzz:
```bash
cargo install cargo-fuzz
```

Run the fuzzer:
```bash
cd fuzz
cargo fuzz run bdk_wallet
```

Run with specific options:
```bash
cargo fuzz run bdk_wallet -- -max_len=10000 -timeout=10
```

## How do I add a new fuzz test target?

1. Define new arbitrary types in `src/arbitrary_types.rs`
2. Implement the `Arbitrary` trait with appropriate constraints
3. Add conversion methods to BDK types
4. Create a new fuzz target in `fuzz_targets/`:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use your_module::YourFuzzInput;

fuzz_target!(|input: YourFuzzInput| {
    // Your fuzzing logic here
});
```

## How do I reproduce a crashing fuzz test?

When the fuzzer finds a crash, it saves the input to `fuzz/artifacts/`. To reproduce:

```bash
cargo fuzz run bdk_wallet fuzz/artifacts/bdk_wallet/crash-<hash>
```

You can also minimize the crash input:
```bash
cargo fuzz tmin bdk_wallet fuzz/artifacts/bdk_wallet/crash-<hash>
```

## Structure-Aware Fuzzing Benefits

Unlike traditional fuzzing that manipulates raw bytes, our structure-aware approach:

1. Generates valid wallet operations by construction
2. Uses weighted probabilities for realistic test scenarios
3. Maintains invariants automatically
4. Provides better code coverage with fewer iterations
5. Makes debugging easier with structured, readable inputs
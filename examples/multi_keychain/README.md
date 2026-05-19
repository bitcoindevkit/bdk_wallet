# New Multi-Keychain API Examples

This directory contains 5 examples to help you understand and explore the new multi-keychain wallet API from a user's perspective. These examples are designed to be read and run in order.

## Examples Overview

### 1. `multi_keychain_migration_guide.rs` - START HERE
**Purpose:** Shows how to think about migrating from the old 2-descriptor API to the new generic API.

**Key Concepts:**
- KeychainKind still works perfectly as a keychain identifier
- The API is now generic over `K` (keychain identifier)
- Explicit keychain construction via `KeyRing`
- New `reveal_next_address(keychain)` pattern vs old `get_address()`

**Run it:** `cargo run --example multi_keychain_migration_guide`

---

### 2. `multi_keychain_persistence.rs`
**Purpose:** Demonstrates complete persistence workflow with multiple keychains.

**Key Concepts:**
- Creating a wallet with `KeyRing` and multiple descriptors
- Persisting to SQLite database
- Loading wallet back with `LoadParams`
- Verification checks (network, descriptors, genesis hash)
- State preservation (address indices are maintained)

**Run it:** `cargo run --example multi_keychain_persistence`

Note: Creates `.bdk_wallet_persistence_example.sqlite` - delete it to run fresh.

---

### 3. `multi_keychain_balance_tracking.rs`
**Purpose:** Shows how to track balances for different keychains separately.

**Key Concepts:**
- Using custom enum as keychain identifier (not just KeychainKind)
- Generating addresses for specific keychains
- `balance()` - total wallet balance
- `keychain_balance(range)` - balance for specific keychain(s)
- Useful for keeping track of segregated funds (savings, spending, donations, etc.)

**Run it:** `cargo run --example multi_keychain_balance_tracking`

---

### 5. `multi_keychain_address_generation.rs`
**Purpose:** Deep dive into address generation with multiple keychains.

**Key Concepts:**
- Each keychain maintains independent address indices
- Sequential vs interleaved address generation
- `reveal_next_address(keychain)` - generates next address for specific keychain
- `next_unused_address(keychain)` - finds first unused address
- `AddressInfo<K>` struct contains address, index, and keychain metadata

**Run it:** `cargo run --example multi_keychain_address_generation`

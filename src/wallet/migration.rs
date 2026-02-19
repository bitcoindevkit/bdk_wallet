// Bitcoin Dev Kit
//
// Copyright (c) 2020-2026 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

//! Migration utilities for upgrading between BDK library versions.
//!
//! This module provides helper functions and types to assist users in migrating wallet data
//! when upgrading between major versions of the `bdk_wallet` crate.

#[cfg(feature = "rusqlite")]
use crate::rusqlite::{self, Connection};
#[cfg(feature = "rusqlite")]
use alloc::{string::String, string::ToString, vec::Vec};

// pre-1.0 sqlite database migration helper functions

/// `Pre1WalletKeychain` represents a structure that holds the keychain details
/// and metadata required for managing a wallet's keys.
#[cfg(feature = "rusqlite")]
#[derive(Debug)]
pub struct Pre1WalletKeychain {
    /// The name of the wallet keychains, "External" or "Internal".
    pub keychain: String,
    /// The index of the last derived key in the wallet keychain.
    pub last_derivation_index: u32,
    /// Checksum of the keychain descriptor, it must match the corresponding post-1.0 bdk wallet
    /// descriptor checksum.
    pub checksum: Vec<u8>,
}

/// Retrieves a list of [`Pre1WalletKeychain`] objects from a pre-1.0 bdk SQLite database.
///
/// This function uses a connection to a pre-1.0 bdk wallet SQLite database to execute a query that
/// retrieves data from two tables (`last_derivation_indices` and `checksums`) and maps the
/// resulting rows to a list of `Pre1WalletKeychain` objects.
#[cfg(feature = "rusqlite")]
pub fn get_pre_1_wallet_keychains(
    conn: &mut Connection,
) -> Result<Vec<Pre1WalletKeychain>, rusqlite::Error> {
    let db_tx = conn.transaction()?;
    let mut statement = db_tx.prepare(
        "SELECT idx.keychain AS keychain, value, checksum FROM last_derivation_indices AS idx \
         JOIN checksums AS chk ON idx.keychain = chk.keychain",
    )?;
    let row_iter = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>("keychain")?,
            row.get::<_, u32>("value")?,
            row.get::<_, Vec<u8>>("checksum")?,
        ))
    })?;
    let mut keychains = vec![];
    for row in row_iter {
        let (keychain, value, checksum) = row?;
        keychains.push(Pre1WalletKeychain {
            keychain: keychain.trim_matches('"').to_string(),
            last_derivation_index: value,
            checksum,
        })
    }
    Ok(keychains)
}

#[cfg(test)]
mod test {
    #[cfg(feature = "rusqlite")]
    use crate::rusqlite::{self, Connection};

    #[cfg(feature = "rusqlite")]
    #[test]
    fn test_get_pre_1_wallet_keychains() -> anyhow::Result<()> {
        let mut conn = Connection::open_in_memory()?;
        let external_checksum = vec![0x01u8, 0x02u8, 0x03u8, 0x04u8];
        let internal_checksum = vec![0x05u8, 0x06u8, 0x07u8, 0x08u8];
        // Init tables
        {
            // Create pre-1.0 bdk sqlite schema
            conn.execute_batch(
                "CREATE TABLE last_derivation_indices (keychain TEXT, value INTEGER);
                 CREATE UNIQUE INDEX idx_indices_keychain ON last_derivation_indices(keychain);
                 CREATE TABLE checksums (keychain TEXT, checksum BLOB);
                 CREATE INDEX idx_checksums_keychain ON checksums(keychain);",
            )?;
            // Insert test data
            conn.execute(
                "INSERT INTO last_derivation_indices (keychain, value) VALUES (?, ?)",
                rusqlite::params!["\"External\"", 42],
            )?;
            conn.execute(
                "INSERT INTO checksums (keychain, checksum) VALUES (?, ?)",
                rusqlite::params!["\"External\"", external_checksum],
            )?;
            conn.execute(
                "INSERT INTO last_derivation_indices (keychain, value) VALUES (?, ?)",
                rusqlite::params!["\"Internal\"", 21],
            )?;
            conn.execute(
                "INSERT INTO checksums (keychain, checksum) VALUES (?, ?)",
                rusqlite::params!["\"Internal\"", internal_checksum],
            )?;
        }

        // test with a 2 keychain wallet
        let result = super::get_pre_1_wallet_keychains(&mut conn)?;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].keychain, "External");
        assert_eq!(result[0].last_derivation_index, 42);
        assert_eq!(result[0].checksum, external_checksum);
        assert_eq!(result[1].keychain, "Internal");
        assert_eq!(result[1].last_derivation_index, 21);
        assert_eq!(result[1].checksum, internal_checksum);
        // delete "Internal" descriptor
        {
            conn.execute(
                "DELETE FROM last_derivation_indices WHERE keychain = ?",
                rusqlite::params!["\"Internal\""],
            )?;
            conn.execute(
                "DELETE FROM checksums WHERE keychain = ?",
                rusqlite::params!["\"Internal\""],
            )?;
        }
        // test with a 1 keychain wallet
        let result = super::get_pre_1_wallet_keychains(&mut conn)?;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].keychain, "External");
        assert_eq!(result[0].last_derivation_index, 42);
        assert_eq!(result[0].checksum, external_checksum);

        Ok(())
    }
}

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

use crate::rusqlite::{self, Connection};
use crate::KeychainKind::{self, External, Internal};
use alloc::{
    string::{FromUtf8Error, String, ToString},
    vec::Vec,
};
use core::fmt;

/// [`PreV1WalletKeychain`] represents a structure that holds the keychain details
/// and metadata required for managing a wallet's keys.
#[derive(Debug)]
pub struct PreV1WalletKeychain {
    /// The name of the wallet keychains, "External" or "Internal".
    pub keychain: KeychainKind,
    /// The index of the last derived key in the wallet keychain.
    pub last_derivation_index: u32,
    /// Checksum of the keychain descriptor, it must match the corresponding post-1.0 bdk wallet
    /// descriptor checksum.
    pub checksum: String,
}

/// Errors thrown when migrating from a pre-v1.0.0 BDK database.
#[derive(Debug)]
pub enum PreV1MigrationError {
    /// A SQLite error
    RusqliteError(rusqlite::Error),
    /// The keychain name is invalid, it must be "External" or "Internal"
    InvalidKeychain(String),
    /// The checksum could not be decoded as utf8
    InvalidChecksum(FromUtf8Error),
}

impl fmt::Display for PreV1MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreV1MigrationError::RusqliteError(e) => write!(f, "Rusqlite error: {}", e),
            PreV1MigrationError::InvalidKeychain(e) => write!(f, "Invalid keychain path: {}", e),
            PreV1MigrationError::InvalidChecksum(e) => write!(f, "Invalid checksum: {}", e),
        }
    }
}

impl std::error::Error for PreV1MigrationError {}

impl From<rusqlite::Error> for PreV1MigrationError {
    fn from(e: rusqlite::Error) -> Self {
        PreV1MigrationError::RusqliteError(e)
    }
}

/// Retrieves a list of [`PreV1WalletKeychain`] objects from a pre-v1.0.0 bdk SQLite database.
///
/// This function uses a connection to a pre-1.0 bdk wallet SQLite database to execute a query that
/// retrieves data from two tables (`last_derivation_indices` and `checksums`) and maps the
/// resulting rows to a list of [`PreV1WalletKeychain`] objects.
pub fn get_pre_v1_wallet_keychains(
    conn: &mut Connection,
) -> Result<Vec<PreV1WalletKeychain>, PreV1MigrationError> {
    let db_tx = conn.transaction()?;
    let mut statement = db_tx
        .prepare(
            "SELECT trim(idx.keychain,'\"') AS keychain, value, checksum FROM last_derivation_indices AS idx \
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
        let keychain = match keychain.as_str() {
            "External" => Ok(External),
            "Internal" => Ok(Internal),
            name => Err(PreV1MigrationError::InvalidKeychain(name.to_string())),
        }?;
        let checksum = String::from_utf8(checksum).map_err(PreV1MigrationError::InvalidChecksum)?;
        keychains.push(PreV1WalletKeychain {
            keychain,
            last_derivation_index: value,
            checksum,
        })
    }
    Ok(keychains)
}

#[cfg(test)]
mod test {
    use crate::rusqlite::{self, Connection};
    use crate::KeychainKind::{External, Internal};

    const SCHEMA_SQL: &str = "CREATE TABLE last_derivation_indices (keychain TEXT, value INTEGER);
                              CREATE UNIQUE INDEX idx_indices_keychain ON last_derivation_indices(keychain);
                              CREATE TABLE checksums (keychain TEXT, checksum BLOB);
                              CREATE INDEX idx_checksums_keychain ON checksums(keychain);";

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn
    }

    fn insert_keychain(
        conn: &Connection,
        keychain: &str,
        value: u32,
        checksum: &[u8],
    ) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO last_derivation_indices (keychain, value) VALUES (?, ?)",
            rusqlite::params![keychain, value],
        )?;
        conn.execute(
            "INSERT INTO checksums (keychain, checksum) VALUES (?, ?)",
            rusqlite::params![keychain, checksum],
        )?;
        Ok(())
    }

    #[test]
    fn test_get_pre_1_wallet_keychains() -> anyhow::Result<()> {
        let mut conn = setup_db();
        let external_checksum = "72k0lrja";
        let internal_checksum = "07nwzkz9";

        insert_keychain(&conn, "\"External\"", 42, external_checksum.as_bytes())?;
        insert_keychain(&conn, "\"Internal\"", 21, internal_checksum.as_bytes())?;

        // test with a 2 keychain wallet
        let result = super::get_pre_v1_wallet_keychains(&mut conn)?;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].keychain, External);
        assert_eq!(result[0].last_derivation_index, 42);
        assert_eq!(result[0].checksum, external_checksum);
        assert_eq!(result[1].keychain, Internal);
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
        let result = super::get_pre_v1_wallet_keychains(&mut conn)?;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].keychain, External);
        assert_eq!(result[0].last_derivation_index, 42);
        assert_eq!(result[0].checksum, external_checksum);

        Ok(())
    }

    #[test]
    fn test_invalid_keychain_name() {
        let mut conn = setup_db();
        insert_keychain(&conn, "\"InvalidKeychain\"", 42, b"72k0lrja").unwrap();

        let result = super::get_pre_v1_wallet_keychains(&mut conn);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, super::PreV1MigrationError::InvalidKeychain(ref name) if name == "InvalidKeychain"),
            "Expected InvalidKeychain error with name 'InvalidKeychain', got: {:?}",
            err
        );
    }

    #[test]
    fn test_invalid_checksum_utf8() {
        let mut conn = setup_db();
        insert_keychain(&conn, "\"External\"", 42, &[0xFF, 0xFE, 0xFD]).unwrap();

        let result = super::get_pre_v1_wallet_keychains(&mut conn);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, super::PreV1MigrationError::InvalidChecksum(_)),
            "Expected InvalidChecksum error, got: {:?}",
            err
        );
    }

    #[test]
    fn test_empty_database() -> anyhow::Result<()> {
        let mut conn = setup_db();
        let result = super::get_pre_v1_wallet_keychains(&mut conn)?;
        assert_eq!(result.len(), 0);
        Ok(())
    }

    #[test]
    fn test_missing_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        let result = super::get_pre_v1_wallet_keychains(&mut conn);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, super::PreV1MigrationError::RusqliteError(_)),
            "Expected RusqliteError, got: {:?}",
            err
        );
    }
}

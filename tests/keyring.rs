use bdk_wallet::descriptor::DescriptorError;
use bdk_wallet::keyring::KeyRing;
use bdk_wallet::KeychainKind;
use bdk_wallet::{chain::DescriptorExt, keyring};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::Network;
use miniscript::{Descriptor, DescriptorPublicKey};

// From the mnemonic "awesome awesome awesome awesome awesome awesome awesome awesome awesome
// awesome awesome awesome"
const DESC_1: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/0/*)";

pub fn get_descriptor(desc_str: &str) -> Descriptor<DescriptorPublicKey> {
    Descriptor::parse_descriptor(&Secp256k1::new(), desc_str)
        .unwrap()
        .0
}

#[test]
fn test_simple_keyring() {
    let network = Network::Regtest;
    let keychain_id = KeychainKind::External;

    let keyring = KeyRing::new(network, keychain_id, DESC_1).unwrap();

    assert_eq!(keyring.default_keychain(), keychain_id);
    assert_eq!(keyring.list_keychains().len(), 1);
}

#[test]
fn test_8_keychains_keyring() {
    const DESC_1: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/0/*)";
    const DESC_2: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/1/*)";
    const DESC_3: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/2/*)";
    const DESC_4: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/3/*)";
    const DESC_5: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/4/*)";
    const DESC_6: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/5/*)";
    const DESC_7: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/6/*)";
    const DESC_8: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/7/*)";

    let mut keyring = KeyRing::new(Network::Regtest, 1, DESC_1).unwrap();
    keyring.add_descriptor(2, DESC_2, false).unwrap();
    keyring.add_descriptor(3, DESC_3, false).unwrap();
    keyring.add_descriptor(4, DESC_4, false).unwrap();
    keyring.add_descriptor(5, DESC_5, false).unwrap();
    keyring.add_descriptor(6, DESC_6, false).unwrap();
    keyring.add_descriptor(7, DESC_7, false).unwrap();
    keyring.add_descriptor(8, DESC_8, false).unwrap();

    assert_eq!(keyring.default_keychain(), 1);
    assert_eq!(keyring.list_keychains().len(), 8);
}

#[test]
fn err_on_hardened_derivation_path() {
    let err =  KeyRing::new(Network::Regtest, KeychainKind::External, "tr(tpubD6NzVbkrYhZ4WyC5VZLuSJQ14uwfUbus7oAFurAFkZA5N3groeQqtW65m8pG1TT1arPpfWu9RbBsc5rSBncrX2d84BAwJJHQfaRjnMCQwuT/86h/1h/0h/0/*)").err();
    assert_eq!(err, Some(DescriptorError::HardenedDerivationXpub));
    let mut keyring = KeyRing::new(Network::Regtest, KeychainKind::External, DESC_1).unwrap();
    let res = keyring.add_descriptor(KeychainKind::Internal,"tr([738b4dbd/86h/1h/0h]tpubDDQsJyQKuP6jCCSZ75Y8zpBAnXsvAN6BWpp6ZoczfxKBDBWnY8XGbC7AMMSyXAcQPNgppkCBmv3hkCLZSaQ4VvSTGsstuTrXuDadMaB7E45/0'/*)", false).err();
    assert_eq!(res, Some(DescriptorError::HardenedDerivationXpub));
}

#[test]
fn err_on_multipath() {
    let err =  KeyRing::new(Network::Regtest, KeychainKind::External, "pkh(tpubD6NzVbkrYhZ4WaWSyoBvQwbpLkojyoTZPRsgXELWz3Popb3qkjcJyJUGLnL4qHHoQvao8ESaAstxYSnhyswJ76uZPStJRJCTKvosUCJZL5B/1/1/<0;1>)").err();
    assert_eq!(
        err,
        Some(DescriptorError::Miniscript(
            miniscript::Error::BadDescriptor(
                "`check_wallet_descriptor` must not contain multipath keys".to_string(),
            )
        ))
    );
    let mut keyring = KeyRing::new(Network::Regtest, KeychainKind::External, DESC_1).unwrap();
    let res = keyring.add_descriptor(KeychainKind::Internal, "tr(tpubD6NzVbkrYhZ4WyC5VZLuSJQ14uwfUbus7oAFurAFkZA5N3groeQqtW65m8pG1TT1arPpfWu9RbBsc5rSBncrX2d84BAwJJHQfaRjnMCQwuT/86/1/0/<0;1>/*)" , false).err();
    assert_eq!(
        res,
        Some(DescriptorError::Miniscript(
            miniscript::Error::BadDescriptor(
                "`check_wallet_descriptor` must not contain multipath keys".to_string(),
            )
        ))
    );
}

#[test]
fn duplicate_desc_keychain() {
    let desc1 = get_descriptor(DESC_1);
    let mut keyring = KeyRing::new(Network::Regtest, desc1.descriptor_id(), desc1.clone()).unwrap();
    let desc2 = get_descriptor("tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/1/*)");
    let res1 = keyring
        .add_descriptor(desc2.descriptor_id(), desc1.clone(), false)
        .err();
    assert_eq!(res1, Some(DescriptorError::DescAlreadyExists));

    let res2 = keyring
        .add_descriptor(desc1.descriptor_id(), desc2, false)
        .err();
    assert_eq!(res2, Some(DescriptorError::KeychainAlreadyExists));
}

#[test]
fn test_persist() {
    use bdk_chain::{rusqlite, DescriptorId};
    use keyring::ChangeSet;
    use tempfile::tempdir;
    let desc1 = get_descriptor(DESC_1);
    let did = desc1.descriptor_id();
    let mut keyring = KeyRing::new(Network::Regtest, did, desc1.clone()).unwrap();
    let changeset = keyring::changeset::ChangeSet {
        network: Some(Network::Regtest),
        descriptors: [(did, desc1)].into(),
        default_keychain: Some(did),
    };
    let dir = tempdir().unwrap();
    let file_path = dir.path().join(".bdk_example_keyring.sqlite");
    let mut conn = rusqlite::Connection::open(file_path).unwrap();
    let db_tx = conn.transaction().unwrap();
    keyring::changeset::ChangeSet::<DescriptorId>::init_sqlite_tables(&db_tx).unwrap();
    changeset.persist_to_sqlite(&db_tx).unwrap();
    db_tx.commit().unwrap();
    let desc2 = get_descriptor("tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/1/*)");
    let changeset2 = keyring
        .add_descriptor(desc2.descriptor_id(), desc2, true)
        .unwrap();
    let db_tx = conn.transaction().unwrap();
    changeset2.persist_to_sqlite(&db_tx).unwrap();
    db_tx.commit().unwrap();
    let db_tx = conn.transaction().unwrap();
    let keyring_read = KeyRing::from_changeset(
        ChangeSet::<DescriptorId>::from_sqlite(&db_tx).unwrap(),
        None,
        [].into(),
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(keyring.list_keychains(), keyring_read.list_keychains());
    assert_eq!(keyring.network(), keyring_read.network());
    assert_eq!(keyring.default_keychain(), keyring_read.default_keychain());
}

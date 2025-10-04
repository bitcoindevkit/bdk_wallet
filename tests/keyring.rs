use bdk_wallet::keyring::KeyRing;
use bdk_wallet::KeychainKind;
use bitcoin::Network;

// From the mnemonic "awesome awesome awesome awesome awesome awesome awesome awesome awesome
// awesome awesome awesome"
const DESC_1: &str = "tr(tprv8ZgxMBicQKsPdWAHbugK2tjtVtRjKGixYVZUdL7xLHMgXZS6BFbFi1UDb1CHT25Z5PU1F9j7wGxwUiRhqz9E3nZRztikGUV6HoRDYcqPhM4/86'/1'/0'/0/*)";

#[test]
fn test_simple_keyring() {
    let network = Network::Regtest;
    let keychain_id = KeychainKind::External;

    let keyring = KeyRing::new(network, keychain_id, DESC_1);

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

    let mut keyring = KeyRing::new(Network::Regtest, 1, DESC_1);
    keyring.add_descriptor(2, DESC_2, false);
    keyring.add_descriptor(3, DESC_3, false);
    keyring.add_descriptor(4, DESC_4, false);
    keyring.add_descriptor(5, DESC_5, false);
    keyring.add_descriptor(6, DESC_6, false);
    keyring.add_descriptor(7, DESC_7, false);
    keyring.add_descriptor(8, DESC_8, false);

    assert_eq!(keyring.default_keychain(), 1);
    assert_eq!(keyring.list_keychains().len(), 8);
}

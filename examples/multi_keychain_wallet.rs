use bdk_wallet::keyring::KeyRing;
use bdk_wallet::Wallet;
use bitcoin::Network;
use miniscript::descriptor::DescriptorType;

// The KeyRing holds a map of keychain identifiers (`K`) to public descriptors. These keychain
// identifiers can be simple (something like the `DescriptorId` or the `KeychainKind` types work
// well, e.g. see the `simple_keyring.rs` example), but they can also be more complex if required by
// the application. This example shows how the keychain identifier can be used to carry metadata
// about the descriptors, which could be used to select which keychain to use in different scenarios
// when calling methods like `Wallet::reveal_next_address`.

// In this example, Johnny has a lot of keychains he keeps track of in his wallet. The map of
// KeychainIdentifier -> Descriptor uses keys that are custom-made at the application layer, and
// useful for its business logic (for example, choose a weekend keychain when Johnny is out partying
// on the weekend).

const DESC_1: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/0/*)#xh44xwsp";
const DESC_2: &str = "wpkh([5bc5d243/84'/1'/0']tpubDCA4DcMLVSDifbfUxyJaVVAx57ztsVjke6DRYF95jFFgJqvzA9oENovVd7n34NNURmZxFNRB1VLGyDEqxvaZNXie3ZroEGFbeTS2xLYuaN1/0/*)#q8afsa3z";
const DESC_3: &str = "pkh([5bc5d243/44'/1'/0']tpubDDNQtvd8Sg1mXtSGtxRWEcgg7PbPwUSAyAmBonDSL3HLuutthe54Yih4XDYcywVdcduwqaQonpbTAGjjSh5kcLeCj5MTjYooa9ve2Npx6ho/1/*)#g73kgtdn";
const DESC_4: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/42/42)";
const DESC_5: &str = "sh(wpkh([5bc5d243/49'/1'/0']tpubDDd6eupua2nhRp2egUAgYGjkxHeh5jPrBDaKLExeySwRvUb1hU7s8osoeACRhXs2w1UGZSMmEpZ1FkjYJ2Pxvfsy7w6XRqYYW7Vw89Unrzr/0/*))#svvvc6el";

fn main() {
    let keychain_1 = KeychainId {
        number: 1,
        nickname: "Johnny's favorite keychain",
        script_type: DescriptorType::Tr,
        color: Color::Blue,
        time_of_week_keychain: DayType::WeekDay,
    };
    let keychain_2 = KeychainId {
        number: 2,
        nickname: "Johnny's party keychain",
        script_type: DescriptorType::Wpkh,
        color: Color::Green,
        time_of_week_keychain: DayType::WeekEnd,
    };
    let keychain_3 = KeychainId {
        number: 3,
        nickname: "Johnny's old P2PKH keychain",
        script_type: DescriptorType::Pkh,
        color: Color::Blue,
        time_of_week_keychain: DayType::AnyDay,
    };
    let keychain_4 = KeychainId {
        number: 4,
        nickname: "Johnny's project donations keychain",
        script_type: DescriptorType::Tr,
        color: Color::Yellow,
        time_of_week_keychain: DayType::AnyDay,
    };
    let keychain_5 = KeychainId {
        number: 5,
        nickname: "The secret keychain",
        script_type: DescriptorType::ShWpkh,
        color: Color::Blue,
        time_of_week_keychain: DayType::AnyDay,
    };

    let mut keyring: KeyRing<KeychainId> =
        KeyRing::new(Network::Signet, keychain_1, DESC_1).unwrap();
    keyring.add_descriptor(keychain_2, DESC_2, false).unwrap();
    keyring.add_descriptor(keychain_3, DESC_3, false).unwrap();
    keyring.add_descriptor(keychain_4, DESC_4, false).unwrap();
    keyring.add_descriptor(keychain_5, DESC_5, false).unwrap();

    // DESC_1 is the default keychain (the first one added to the keyring is automatically the
    // default keychain), but this can also be changed later on with the
    // KeyRing::set_default_keychain API. This default keychain is useful because you can use
    // APIs like `Wallet::reveal_next_default_address()` which will always work with your
    // default keychain.

    let mut wallet = Wallet::new(keyring);

    let address1 = wallet.reveal_next_default_address();
    println!("Default keychain address: {}", address1.address);

    let party_address = wallet.reveal_next_address(keychain_2).unwrap();
    println!("Party address:            {}", party_address.address);

    let donation_address = wallet.reveal_next_address(keychain_4).unwrap();
    println!("Donation address:         {}", donation_address.address);
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct KeychainId {
    number: u32,
    nickname: &'static str,
    script_type: DescriptorType,
    color: Color,
    time_of_week_keychain: DayType,
}

impl PartialEq for KeychainId {
    fn eq(&self, other: &Self) -> bool {
        self.number == other.number
    }
}

impl Eq for KeychainId {}

impl PartialOrd for KeychainId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for KeychainId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.number.cmp(&other.number)
    }
}

#[derive(Debug, Clone, Copy)]
enum Color {
    Blue,
    Green,
    Yellow,
}

#[derive(Debug, Clone, Copy)]
enum DayType {
    AnyDay,
    WeekDay,
    WeekEnd,
}

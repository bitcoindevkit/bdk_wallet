// Adapted from bitcoindevkit/bdk_wallet#318 (examples/multi_keychain/wallet.rs).
//
// A `Wallet<K>` tracks a map of keychain identifiers (`K`) to descriptors. `K` can be something
// simple like `KeychainKind`, but it can also be a type of your own that carries whatever metadata
// your application needs. The wallet only ever requires `K: Ord + Clone + Debug`, so the rest of
// the type is yours.
//
// Here Johnny keeps several keychains in one wallet, and the identifier records what each one is
// for — which is what lets the application decide which keychain to reveal an address from.

use std::cmp::Ordering;

use bdk_wallet::bitcoin::Network;
use bdk_wallet::miniscript::descriptor::DescriptorType;
use bdk_wallet::{KeyRing, Wallet};

const DESC_1: &str = "tr([5bc5d243/86'/1'/0']tpubDC72NVP1RK5qwy2QdEfWphDsUBAfBu7oiV6jEFooHP8tGQGFVUeFxhgZxuk1j6EQRJ1YsS3th2RyDgReRqCL4zqp4jtuV2z7gbiqDH2iyUS/0/*)#xh44xwsp";
const DESC_2: &str = "wpkh([5bc5d243/84'/1'/0']tpubDCA4DcMLVSDifbfUxyJaVVAx57ztsVjke6DRYF95jFFgJqvzA9oENovVd7n34NNURmZxFNRB1VLGyDEqxvaZNXie3ZroEGFbeTS2xLYuaN1/0/*)#q8afsa3z";
const DESC_3: &str = "pkh([5bc5d243/44'/1'/0']tpubDDNQtvd8Sg1mXtSGtxRWEcgg7PbPwUSAyAmBonDSL3HLuutthe54Yih4XDYcywVdcduwqaQonpbTAGjjSh5kcLeCj5MTjYooa9ve2Npx6ho/1/*)#g73kgtdn";
const DESC_4: &str = "sh(wpkh([5bc5d243/49'/1'/0']tpubDDd6eupua2nhRp2egUAgYGjkxHeh5jPrBDaKLExeySwRvUb1hU7s8osoeACRhXs2w1UGZSMmEpZ1FkjYJ2Pxvfsy7w6XRqYYW7Vw89Unrzr/0/*))#svvvc6el";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let everyday = KeychainId {
        number: 1,
        nickname: "Johnny's favorite keychain",
        script_type: DescriptorType::Tr,
        day_type: DayType::WeekDay,
    };
    let party = KeychainId {
        number: 2,
        nickname: "Johnny's party keychain",
        script_type: DescriptorType::Wpkh,
        day_type: DayType::WeekEnd,
    };
    let legacy = KeychainId {
        number: 3,
        nickname: "Johnny's old P2PKH keychain",
        script_type: DescriptorType::Pkh,
        day_type: DayType::AnyDay,
    };
    let donations = KeychainId {
        number: 4,
        nickname: "Johnny's project donations keychain",
        script_type: DescriptorType::ShWpkh,
        day_type: DayType::AnyDay,
    };

    // A `KeyRing` is built one keychain at a time. Every descriptor is validated against the
    // network as it goes in, and duplicate keychains or duplicate descriptors are rejected here —
    // which is why building the wallet from it afterwards cannot fail.
    let mut keyring = KeyRing::new(Network::Signet, everyday, DESC_1)?;
    keyring.add_descriptor(party, DESC_2)?;
    keyring.add_descriptor(legacy, DESC_3)?;
    keyring.add_descriptor(donations, DESC_4)?;

    let mut wallet = Wallet::create(keyring).create_wallet_no_persist();

    println!(
        "This wallet tracks {} keychains:",
        wallet.keychains().count()
    );
    for (keychain, _descriptor) in wallet.keychains() {
        println!(
            "  {}. {:?}, used {:?}  ({})",
            keychain.number, keychain.script_type, keychain.day_type, keychain.nickname
        );
    }

    // Because the identifier carries the metadata, picking a keychain is ordinary application
    // logic rather than something the wallet has to model.
    println!();
    let party_address = wallet.reveal_next_address(party);
    println!("Party address:     {}", party_address.address);

    let donation_address = wallet.reveal_next_address(donations);
    println!("Donation address:  {}", donation_address.address);

    Ok(())
}

/// Johnny's keychain identifier.
///
/// Only `number` participates in equality and ordering: it is the identity of the keychain, and the
/// rest is metadata hanging off it. Note that ordering also decides the order keychains come back
/// in from [`Wallet::keychains`], since they are held in a `BTreeMap`.
#[derive(Debug, Clone, Copy)]
struct KeychainId {
    number: u32,
    nickname: &'static str,
    script_type: DescriptorType,
    day_type: DayType,
}

impl PartialEq for KeychainId {
    fn eq(&self, other: &Self) -> bool {
        self.number == other.number
    }
}

impl Eq for KeychainId {}

impl PartialOrd for KeychainId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for KeychainId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.number.cmp(&other.number)
    }
}

#[derive(Debug, Clone, Copy)]
enum DayType {
    AnyDay,
    WeekDay,
    WeekEnd,
}

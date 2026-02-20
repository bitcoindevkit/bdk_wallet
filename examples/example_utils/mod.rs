#![allow(dead_code)]

use bitcoin::secp256k1::Secp256k1;
use miniscript::{Descriptor, DescriptorPublicKey};

pub const DESCRIPTORS: [&str; 8] = [
    "pkh([21a559b8/44h/1h/0h]tpubDD2jQhEsAU9uU6Qec2M6k5ygyLS96cCa9iwpQrGAS7p6GRW7eVjCiy1WGwTxqCPiACg99A4vUCZWs5w3xgHgTohxYu2z1ZzhLrLaFHG8at1/0/*)#8t439hyk",
    "pkh([21a559b8/44h/1h/0h]tpubDD2jQhEsAU9uU6Qec2M6k5ygyLS96cCa9iwpQrGAS7p6GRW7eVjCiy1WGwTxqCPiACg99A4vUCZWs5w3xgHgTohxYu2z1ZzhLrLaFHG8at1/1/*)#klsscz5w",
    "sh(wpkh([21a559b8/49h/1h/0h]tpubDCpBeRAYSsoF8LkbeSa1joNjNPMgTk8jttvydCXTxDwKHJsQab4pr6NeHn11fTKBjHE7NmHrUjEgH3mkQZrQ89qy9tLpZhpMc9w6NvkKkFL/0/*))#5adnu2fe",
    "sh(wpkh([21a559b8/49h/1h/0h]tpubDCpBeRAYSsoF8LkbeSa1joNjNPMgTk8jttvydCXTxDwKHJsQab4pr6NeHn11fTKBjHE7NmHrUjEgH3mkQZrQ89qy9tLpZhpMc9w6NvkKkFL/1/*))#pur9y4ux",
    "tr([21a559b8/86h/1h/0h]tpubDCdJ64usD8SEL17thc2K43vG4zHNMdgvnRWNvD3j2bEGCh2ER4KjJN3zhhD54AkBuc5k695JsgkTtdVaARepr2kNxv83zKP5Ra6CsUqoPuu/0/*)#2sd7vzr2",
    "tr([21a559b8/86h/1h/0h]tpubDCdJ64usD8SEL17thc2K43vG4zHNMdgvnRWNvD3j2bEGCh2ER4KjJN3zhhD54AkBuc5k695JsgkTtdVaARepr2kNxv83zKP5Ra6CsUqoPuu/1/*)#mygl3hnj",
    "wpkh([21a559b8/84h/1h/0h]tpubDCWfvMa9sCes3z6nxkF7ox5kc6gjddkkJJHTJKfnj96uRGXDWY9WbpNxupEYREV4kHij4JUBYzp9ziM7qhvsUjcisQHaSQgya39nDvuimkF/0/*)#7gmvfd4d",
    "wpkh([21a559b8/84h/1h/0h]tpubDCWfvMa9sCes3z6nxkF7ox5kc6gjddkkJJHTJKfnj96uRGXDWY9WbpNxupEYREV4kHij4JUBYzp9ziM7qhvsUjcisQHaSQgya39nDvuimkF/1/*)#0u7d5c94"
];

pub fn get_descriptor(desc_str: &str) -> Descriptor<DescriptorPublicKey> {
    Descriptor::parse_descriptor(&Secp256k1::new(), desc_str)
        .unwrap()
        .0
}

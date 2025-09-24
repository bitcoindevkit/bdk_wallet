use async_hwi::bitbox::api::runtime::TokioRuntime;
use async_hwi::bitbox::api::BitBox;
use async_hwi::bitbox::NoiseConfigNoCache;
use bdk_wallet::bitcoin::absolute::LockTime;
use bdk_wallet::bitcoin::{Amount, FeeRate, Network};

use bdk_wallet::test_utils::get_funded_wallet;
use async_hwi::{bitbox::BitBox02, HWI};
use bdk_wallet::KeychainKind;

const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const NETWORK: Network = Network::Regtest;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdfCLpvozodGytD3gRUa1M5WQz4kNuDZVf1inhcsSHXRpyLWN3k3Qy3nucrzz5hw2iZiEs6spehpee2WxqfSi31ByRJEu4rZ/84h/1h/0h/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdfCLpvozodGytD3gRUa1M5WQz4kNuDZVf1inhcsSHXRpyLWN3k3Qy3nucrzz5hw2iZiEs6spehpee2WxqfSi31ByRJEu4rZ/84h/1h/0h/1/*)";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let (mut wallet, _) = get_funded_wallet(EXTERNAL_DESC, INTERNAL_DESC);

    // Pairing with Bitbox connected Bitbox device
    let noise_config = Box::new(NoiseConfigNoCache {});

    let bitbox = {
        #[cfg(feature = "simulator")]
        {
            BitBox::<TokioRuntime>::from_simulator(None, noise_config).await?
        }

        #[cfg(not(feature = "simulator"))]
        {
            use async_hwi::bitbox::api::usb;
            BitBox::<TokioRuntime>::from_hid_device(usb::get_any_bitbox02().unwrap(), noise_config)
                .await?
        }
    };

    let pairing_device = bitbox.unlock_and_pair().await?;
    let paired_device = pairing_device.wait_confirm().await?;

    if let Ok(_) = paired_device.restore_from_mnemonic().await {
        println!("Initializing device with mnemonic...");
    } else {
        println!("Device already initialized proceeding...");
    }

    let bb = BitBox02::from(paired_device);
    let bb = bb.with_network(NETWORK);

    let receiving_address = wallet.next_unused_address(KeychainKind::External);

    println!("Wallet balance {}", wallet.balance());

    let mut tx_builder = wallet.build_tx();

    tx_builder
        .add_recipient(receiving_address.script_pubkey(), SEND_AMOUNT)
        .fee_rate(FeeRate::from_sat_per_vb(2).unwrap())
        .nlocktime(LockTime::from_height(0).unwrap());

    let mut psbt = tx_builder.finish()?;

    // Sign with the connected bitbox or any hardware device
    bb.sign_tx(&mut psbt).await?;
    
    println!("Signing with bitbox done. Balance After signing {}", wallet.balance());
    Ok(())
}

use std::sync::Arc;

use async_hwi::bitbox::api::runtime::TokioRuntime;
use async_hwi::bitbox::api::{usb, BitBox};
use async_hwi::bitbox::NoiseConfigNoCache;
use bdk_wallet::bitcoin::secp256k1::{All, Secp256k1};
use bdk_wallet::bitcoin::{Network, Psbt};
use bdk_wallet::signer::{SignerError, SignerOrdering, TransactionSigner};
use bdk_wallet::KeychainKind;
use bdk_wallet::{signer::SignerCommon, signer::SignerId, Wallet};

use async_hwi::{bitbox::BitBox02, HWI};
use tokio::runtime::Runtime;

#[derive(Debug)]
struct HwiSigner<T: HWI> {
    hw_device: T,
}

impl<T: HWI> HwiSigner<T> {
    async fn sign_tx(&self, psbt: &mut Psbt) -> Result<(), SignerError> {
        if let Err(e) = self.hw_device.sign_tx(psbt).await {
            return Err(SignerError::External(e.to_string()));
        }
        Ok(())
    }

    fn new(hw_device: T) -> Self {
        HwiSigner { hw_device }
    }

    fn get_id(&self) -> SignerId {
        SignerId::Dummy(0)
    }
}

impl<T> SignerCommon for HwiSigner<T>
where
    T: Sync + Send + HWI,
{
    fn id(&self, _secp: &Secp256k1<All>) -> SignerId {
        self.get_id()
    }
}

impl<T> TransactionSigner for HwiSigner<T>
where
    T: Sync + Send + HWI,
{
    fn sign_transaction(
        &self,
        psbt: &mut Psbt,
        _sign_options: &bdk_wallet::SignOptions,
        _secp: &Secp256k1<All>,
    ) -> Result<(), SignerError> {
        let rt = Runtime::new().unwrap();
        rt.block_on(self.sign_tx(psbt))?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let descriptor = "wpkh(tpubD6NzVbkrYhZ4Xferm7Pz4VnjdcDPFyjVu5K4iZXQ4pVN8Cks4pHVowTBXBKRhX64pkRyJZJN5xAKj4UDNnLPb5p2sSKXhewoYx5GbTdUFWq/0/*)";
    let change_descriptor = "wpkh(tpubD6NzVbkrYhZ4Xferm7Pz4VnjdcDPFyjVu5K4iZXQ4pVN8Cks4pHVowTBXBKRhX64pkRyJZJN5xAKj4UDNnLPb5p2sSKXhewoYx5GbTdUFWq/1/*)";

    let noise_config = Box::new(NoiseConfigNoCache {});
    let bitbox =
        BitBox::<TokioRuntime>::from_hid_device(usb::get_any_bitbox02().unwrap(), noise_config)
            .await?;

    let pairing_device = bitbox.unlock_and_pair().await?;
    let paired_device = pairing_device.wait_confirm().await?;
    let bb = BitBox02::from(paired_device);

    let _ = bb.register_wallet("test-wallet", descriptor).await.unwrap();

    let bitbox_signer = HwiSigner::new(bb);

    let mut wallet = Wallet::create(descriptor, change_descriptor)
        .network(Network::Testnet)
        .create_wallet_no_persist()?;

    wallet.add_signer(
        KeychainKind::External,
        SignerOrdering(100),
        Arc::new(bitbox_signer),
    );

    Ok(())
}

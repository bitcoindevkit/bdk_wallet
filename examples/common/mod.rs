//! Common signing utilities for bdk_wallet examples
//!
//! This module provides the `SignerWrapper` struct and related utilities
//! that enable signing functionality for the wallet examples. These utilities
//! wrap the KeyMap type to implement the GetKey trait, allowing examples
//! to sign transactions and PSBTs.
//!
//! Note: This module is only required temporarily until miniscript 12.x is released,
//! which will include signing capabilities for KeyMap natively.

use miniscript::descriptor::{DescriptorSecretKey, KeyMap};
use std::collections::BTreeMap;

use bitcoin::{
    key::Secp256k1,
    psbt::{GetKey, GetKeyError, KeyRequest},
};

#[derive(Debug, Clone)]
/// A wrapper over the [`KeyMap`] type that has the `GetKey` trait implementation for signing.
pub struct SignerWrapper {
    key_map: KeyMap,
}

impl SignerWrapper {
    /// Creates a new [`SignerWrapper`] for the given [`KeyMap`].
    pub fn new(key_map: KeyMap) -> Self {
        Self { key_map }
    }
}

impl GetKey for SignerWrapper {
    type Error = GetKeyError;

    fn get_key<C: bitcoin::secp256k1::Signing>(
        &self,
        key_request: KeyRequest,
        secp: &bitcoin::key::Secp256k1<C>,
    ) -> Result<Option<bitcoin::PrivateKey>, Self::Error> {
        for key_map in self.key_map.iter() {
            let (_, desc_sk) = key_map;
            let wrapper = DescriptorSecretKeyWrapper::new(desc_sk.clone());
            match wrapper.get_key(key_request.clone(), secp) {
                Ok(Some(private_key)) => return Ok(Some(private_key)),
                Ok(None) => continue,
                // TODO: (@leonardo) how should we handle this ?
                // we can't error-out on this, because the valid signing key can be in the next
                // iterations.
                Err(_) => continue,
            }
        }
        Ok(None)
    }
}

/// Wrapper for DescriptorSecretKey to implement GetKey trait
pub struct DescriptorSecretKeyWrapper(DescriptorSecretKey);

impl DescriptorSecretKeyWrapper {
    /// Creates a new DescriptorSecretKeyWrapper
    pub fn new(desc_sk: DescriptorSecretKey) -> Self {
        Self(desc_sk)
    }
}

impl GetKey for DescriptorSecretKeyWrapper {
    type Error = GetKeyError;

    fn get_key<C: bitcoin::secp256k1::Signing>(
        &self,
        key_request: KeyRequest,
        secp: &Secp256k1<C>,
    ) -> Result<Option<bitcoin::PrivateKey>, Self::Error> {
        match (&self.0, key_request) {
            (DescriptorSecretKey::Single(single_priv), key_request) => {
                let private_key = single_priv.key;
                let public_key = private_key.public_key(secp);
                let pubkey_map = BTreeMap::from([(public_key, private_key)]);
                return pubkey_map.get_key(key_request, secp);
            }
            (DescriptorSecretKey::XPrv(descriptor_xkey), KeyRequest::Pubkey(public_key)) => {
                let private_key = descriptor_xkey.xkey.private_key;
                let pk = private_key.public_key(secp);
                if public_key.inner.eq(&pk) {
                    return Ok(Some(
                        descriptor_xkey
                            .xkey
                            .derive_priv(secp, &descriptor_xkey.derivation_path)
                            .map_err(GetKeyError::Bip32)?
                            .to_priv(),
                    ));
                }
            }
            (
                DescriptorSecretKey::XPrv(descriptor_xkey),
                ref key_request @ KeyRequest::Bip32(ref key_source),
            ) => {
                if let Some(key) = descriptor_xkey.xkey.get_key(key_request.clone(), secp)? {
                    return Ok(Some(key));
                }

                if let Some(_derivation_path) = descriptor_xkey.matches(key_source, secp) {
                    let (_fp, derivation_path) = key_source;

                    if let Some((_fp, origin_derivation_path)) = &descriptor_xkey.origin {
                        let derivation_path = &derivation_path[origin_derivation_path.len()..];
                        return Ok(Some(
                            descriptor_xkey
                                .xkey
                                .derive_priv(secp, &derivation_path)
                                .map_err(GetKeyError::Bip32)?
                                .to_priv(),
                        ));
                    } else {
                        return Ok(Some(
                            descriptor_xkey
                                .xkey
                                .derive_priv(secp, derivation_path)
                                .map_err(GetKeyError::Bip32)?
                                .to_priv(),
                        ));
                    };
                }
            }
            (DescriptorSecretKey::XPrv(_), KeyRequest::XOnlyPubkey(_)) => {
                return Err(GetKeyError::NotSupported)
            }
            (DescriptorSecretKey::MultiXPrv(_), _) => unimplemented!(),
            _ => unreachable!(),
        }
        Ok(None)
    }
}

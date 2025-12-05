// Bitcoin Dev Kit
// Written in 2020 by Alekos Filini <alekos.filini@gmail.com>
//
// Copyright (c) 2020-2021 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use core::ops::{Deref, DerefMut};

use bitcoin::Amount;
use bitcoin_payment_instructions::{hrn_resolution::HrnResolver, PaymentInstructions};

use crate::tx_builder::TxBuilder;

/// A transaction builder with BIP 353 DNS payment instructions support
#[derive(Debug)]
pub struct TxBuilderDns<'a, Cs, R> {
    pub(crate) tx_builder: TxBuilder<'a, Cs>,
    pub(crate) resolver: R,
}

impl<'a, Cs, R> Deref for TxBuilderDns<'a, Cs, R> {
    type Target = TxBuilder<'a, Cs>;
    fn deref(&self) -> &Self::Target {
        &self.tx_builder
    }
}

impl<'a, Cs, R> DerefMut for TxBuilderDns<'a, Cs, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tx_builder
    }
}

impl<'a, Cs, R: HrnResolver> TxBuilderDns<'a, Cs, R> {
    /// Chose the resolver to use
    pub fn resolver<Rs: HrnResolver>(self, resolver: Rs) -> TxBuilderDns<'a, Cs, Rs> {
        TxBuilderDns {
            tx_builder: self.tx_builder,
            resolver,
        }
    }

    // Add a recipient with human_readable_name to the internal list
    // The human readable name is in the form ₿user@domain or user@domain
    pub fn add_recipient(
        &mut self,
        human_readable_name: &str,
        amount: Amount,
    ) -> Result<&mut Self, &str> {

        Ok(self)
    }
}

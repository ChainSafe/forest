// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr as _;

use crate::rpc::{self, prelude::*};
use crate::shim::address::{Address, StrictAddress};
use crate::shim::econ::TokenAmount;
use crate::shim::message::{Message, METHOD_SEND};
use anyhow::Context as _;
use num::Zero as _;

use crate::cli::humantoken;

#[derive(Debug, clap::Args)]
pub struct SendCommand {
    /// optionally specify the account to send funds from (otherwise the default
    /// one will be used)
    #[arg(long)]
    from: Option<String>,
    target_address: String,
    #[arg(value_parser = humantoken::parse)]
    amount: TokenAmount,
    #[arg(long, value_parser = humantoken::parse, default_value_t = TokenAmount::zero())]
    gas_feecap: TokenAmount,
    /// In milliGas
    #[arg(long, default_value_t = 0)]
    gas_limit: i64,
    #[arg(long, value_parser = humantoken::parse, default_value_t = TokenAmount::zero())]
    gas_premium: TokenAmount,
}

impl SendCommand {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        eprintln!(
            "This command has been deprecated and will be removed in the future.\n\
             Please use the 'forest-wallet' executable instead."
        );

        let from: Address = if let Some(from) = &self.from {
            StrictAddress::from_str(from)?.into()
        } else {
            WalletDefaultAddress::call(&client, ())
                .await?
                .context("No default wallet address selected. Please set a default address.")?
        };

        let message = Message {
            from,
            to: StrictAddress::from_str(&self.target_address)?.into(),
            value: self.amount.clone(),
            method_num: METHOD_SEND,
            gas_limit: self.gas_limit as u64,
            gas_fee_cap: self.gas_feecap.clone(),
            gas_premium: self.gas_premium.clone(),
            ..Default::default()
        };

        let signed_msg = MpoolPushMessage::call(&client, (message, None)).await?;

        println!("{}", signed_msg.cid());

        Ok(())
    }
}

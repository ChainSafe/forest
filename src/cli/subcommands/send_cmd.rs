// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use crate::lotus_json::LotusJson;
use crate::rpc_client::{mpool_push_message, wallet_default_address};
use crate::shim::address::{Address, StrictAddress};
use crate::shim::econ::TokenAmount;
use crate::shim::message::{Message, METHOD_SEND};
use num::Zero as _;

use super::{handle_rpc_err, Config};
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
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        let from: Address = if let Some(from) = &self.from {
            StrictAddress::from_str(from)?.into()
        } else {
            Address::from_str(
                &wallet_default_address((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "No default wallet address selected. Please set a default address."
                        )
                    })?,
            )?
        };

        let message = Message {
            from,
            to: StrictAddress::from_str(&self.target_address)?.into(),
            value: self.amount.clone(),
            method_num: METHOD_SEND,
            gas_limit: self.gas_limit as u64,
            gas_fee_cap: self.gas_feecap.clone(),
            gas_premium: self.gas_premium.clone(),
            // JANK(aatifsyed): Why are we using a testing build of fvm_shared?
            ..Default::default()
        };

        let signed_msg = mpool_push_message((LotusJson(message), None), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?
            .into_inner();

        println!("{}", signed_msg.cid().unwrap());

        Ok(())
    }
}

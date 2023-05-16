// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use forest_json::message::json::MessageJson;
use forest_rpc_client::{mpool_push_message, wallet_default_address};
use forest_shim::econ::TokenAmount;
use fvm_ipld_encoding::Cbor;
use fvm_shared::{address::Address, message::Message, METHOD_SEND};
use num::Zero as _;

use super::{handle_rpc_err, Config};
use crate::humantoken;

#[derive(Debug, clap::Args)]
pub struct SendCommand {
    /// optionally specify the account to send funds from (otherwise the default
    /// one will be used)
    #[arg(long)]
    from: Option<Address>,
    target_address: Address,
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
        let from: Address = if let Some(from) = self.from {
            from
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
            to: self.target_address,
            value: self.amount.clone().into(),
            method_num: METHOD_SEND,
            gas_limit: self.gas_limit,
            gas_fee_cap: self.gas_feecap.clone().into(),
            gas_premium: self.gas_premium.clone().into(),
            // JANK(aatifsyed): Why are we using a testing build of fvm_shared?
            ..Default::default()
        };

        let signed_msg_json = mpool_push_message(
            (MessageJson(message.into()), None),
            &config.client.rpc_token,
        )
        .await
        .map_err(handle_rpc_err)?;

        println!("{}", signed_msg_json.0.cid().unwrap());

        Ok(())
    }
}

// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_message::message::json::MessageJson;
use forest_rpc_client::{mpool_push_message, wallet_default_address};
use fvm_shared::{
    address::Address, bigint::BigInt, econ::TokenAmount, message::Message, METHOD_SEND,
};
use std::str::FromStr;
use structopt::StructOpt;

use super::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub struct SendCommand {
    /// optionally specify the account to send funds from (otherwise the default one will be used)
    #[structopt(long)]
    from: Option<Address>,
    target_address: Address,
    /// token amount in attoFIL
    amount: BigInt,
    /// specify gas fee cap to use in attoFIL
    #[structopt(long)]
    gas_feecap: Option<BigInt>,
    /// specify gas limit in attoFIL
    #[structopt(long)]
    gas_limit: Option<i64>,
    /// specify gas price to use in attoFIL
    #[structopt(long)]
    gas_premium: Option<BigInt>,
}

impl SendCommand {
    pub async fn run(&self) -> anyhow::Result<()> {
        let from: Address = if let Some(from) = self.from {
            from
        } else {
            Address::from_str(&wallet_default_address().await.map_err(handle_rpc_err)?)?
        };

        let message = Message {
            from,
            to: self.target_address,
            value: TokenAmount::from_atto(self.amount.clone()),
            method_num: METHOD_SEND,
            gas_limit: self.gas_limit.unwrap_or_default(),
            gas_fee_cap: TokenAmount::from_atto(self.gas_feecap.clone().unwrap_or_default()),
            gas_premium: TokenAmount::from_atto(self.gas_premium.clone().unwrap_or_default()),
            ..Default::default()
        };

        mpool_push_message((MessageJson(message), None))
            .await
            .map_err(handle_rpc_err)?;

        Ok(())
    }
}

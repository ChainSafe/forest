// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use forest_json::message::json::MessageJson;
use forest_rpc_client::{mpool_push_message, wallet_default_address};
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, METHOD_SEND};
use lazy_static::lazy_static;
use num::{rational::Ratio, BigInt, CheckedMul};
use regex::Regex;

use super::{handle_rpc_err, Config};

lazy_static! {
    static ref FIL_REG: Regex = Regex::new(r"^(?:\d*\.)?\d+").unwrap();
}

const FILECOIN_PRECISION: i64 = 1_000_000_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FILAmount {
    value: BigInt,
    units: String,
}

impl FromStr for FILAmount {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let suffix = FIL_REG.replace(s, "");
        let val = s.trim_end_matches(&suffix.to_string());
        let mut is_attofil = false;

        if !suffix.is_empty() {
            let suffix_match = suffix.trim().to_lowercase();
            match suffix_match.as_str() {
                "attofil" | "afil" => {
                    is_attofil = true;
                }
                "fil" | "" => {}
                _ => {
                    return Err(anyhow::anyhow!("unrecognized suffix: {}", suffix_match));
                }
            }
        }

        if val.chars().count() > 50 {
            return Err(anyhow::anyhow!(
                "string length too large: {}",
                val.chars().count()
            ));
        }

        let mut r = if Ratio::from_float(val.parse::<f64>().unwrap()).is_none() {
            return Err(anyhow::anyhow!(
                "failed to parse {} as a decimal number",
                val
            ));
        } else {
            Ratio::from_float(val.parse::<f64>().unwrap()).unwrap()
        };

        if !is_attofil {
            r = Ratio::checked_mul(&r, &Ratio::new(FILECOIN_PRECISION.into(), BigInt::from(1)))
                .unwrap();
        }

        if !Ratio::is_integer(&r) {
            let mut prefix = "";
            if is_attofil {
                prefix = "atto";
            }
            return Err(anyhow::anyhow!("invalid {}FIL value: {}", prefix, val));
        }

        //TODO: update fields when finished with this section
        Ok(FILAmount {
            value: val.parse::<BigInt>().unwrap(),
            units: "".to_string(),
        })
    }
}

#[derive(Debug, clap::Args)]
pub struct SendCommand {
    /// optionally specify the account to send funds from (otherwise the default
    /// one will be used)
    #[arg(long)]
    from: Option<Address>,
    target_address: Address,
    /// token amount in attoFIL
    amount: FILAmount,
    /// specify gas fee cap to use in attoFIL
    #[arg(long)]
    gas_feecap: Option<BigInt>,
    /// specify gas limit in attoFIL
    #[arg(long)]
    gas_limit: Option<i64>,
    /// specify gas price to use in attoFIL
    #[arg(long)]
    gas_premium: Option<BigInt>,
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

        // let amount = if self.amount ==  {

        // };

        let message = Message {
            from,
            to: self.target_address,
            value: TokenAmount::from_atto(self.amount.value.clone()),
            method_num: METHOD_SEND,
            gas_limit: self.gas_limit.unwrap_or_default(),
            gas_fee_cap: TokenAmount::from_atto(self.gas_feecap.clone().unwrap_or_default()),
            gas_premium: TokenAmount::from_atto(self.gas_premium.clone().unwrap_or_default()),
            ..Default::default()
        };

        mpool_push_message(
            (MessageJson(message.into()), None),
            &config.client.rpc_token,
        )
        .await
        .map_err(handle_rpc_err)?;

        Ok(())
    }
}

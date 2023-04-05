// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use forest_json::message::json::MessageJson;
use forest_rpc_client::{mpool_push_message, wallet_default_address};
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, METHOD_SEND};
use num::BigInt;
use rust_decimal::prelude::*;

use super::{handle_rpc_err, Config};

#[derive(Debug, Clone, PartialEq, Eq)]
struct FILAmount {
    value: TokenAmount,
}

impl FromStr for FILAmount {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let suffix_idx = s.rfind(char::is_numeric);
        let (val, suffix) = match suffix_idx {
            Some(idx) => s.split_at(idx + 1),
            None => return Err(anyhow::anyhow!("failed to parse string: {}", s)),
        };
        let mut is_attofil = false;

        if !suffix.is_empty() {
            match suffix.trim().to_lowercase().strip_suffix("fil") {
                Some("atto" | "a") => {
                    is_attofil = true;
                }
                Some("femto" | "pico" | "nano" | "micro" | "milli" | "" | " ") => {}
                _ => {
                    return Err(anyhow::anyhow!("unrecognized suffix: {}", suffix));
                }
            }
        }

        if val.chars().count() > 50 {
            return Err(anyhow::anyhow!(
                "string length too large: {}",
                val.chars().count()
            ));
        }

        let parsed_val = match val.parse::<f64>() {
            Ok(value) => value,
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "failed to parse {} as a decimal number",
                    val
                ))
            }
        };

        let token_amount = if is_attofil && parsed_val.fract() != 0.0 {
            return Err(anyhow::anyhow!("invalid attoFIL value: {}", val));
        } else if is_attofil {
            TokenAmount::from_atto(BigInt::from_f64(parsed_val).unwrap())
        } else {
            match suffix.trim().to_lowercase().strip_suffix("fil") {
                Some("femto") => {
                    TokenAmount::from_atto(BigInt::from_f64(parsed_val * 1_000.0).unwrap())
                }
                Some("pico") => {
                    TokenAmount::from_atto(BigInt::from_f64(parsed_val * 1_000_000.0).unwrap())
                }
                Some("nano") => {
                    TokenAmount::from_atto(BigInt::from_f64(parsed_val * 1_000_000_000.0).unwrap())
                }
                Some("micro") => TokenAmount::from_atto(
                    BigInt::from_f64(parsed_val * 1_000_000_000_000.0).unwrap(),
                ),
                Some("milli") => TokenAmount::from_atto(
                    BigInt::from_f64(parsed_val * 1_000_000_000_000_000.0).unwrap(),
                ),
                _ => TokenAmount::from_atto(
                    BigInt::from_f64(parsed_val * 1_000_000_000_000_000_000.0).unwrap(),
                ),
            }
        };

        Ok(FILAmount {
            value: token_amount,
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

        //TODO: update value field and update integration tests
        let message = Message {
            from,
            to: self.target_address,
            value: self.amount.value.clone(),
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

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::*;
    use crate::cli::wallet_cmd::{bool_pair_to_mode, format_balance_string};
    const FILECOIN_PRECISION: u64 = 1_000_000_000_000_000_000;

    #[test]
    fn invalid_attofil_amount() {
        //attoFIL with fractional value fails (fractional FIL values allowed)
        let amount = "1.234attofil";
        assert!(FILAmount::from_str(amount).is_err());
    }

    #[test]
    fn valid_attofil_amount() {
        //valid attofil amount passes
        let amount = "1234 attofil";
        assert!(FILAmount::from_str(amount).is_ok());
    }

    #[test]
    fn suffix_with_no_amount() {
        //fails if no amount specified
        let amount = "fil";
        assert!(FILAmount::from_str(amount).is_err());
    }
    #[test]
    fn valid_fil_amount_without_suffix() {
        //defaults to FIL if no suffix is provided
        let amount = "1234";
        assert!(FILAmount::from_str(amount).is_ok());
    }

    #[test]
    fn valid_fil_amount_with_suffix() {
        //properly parses amount with "FIL" suffix
        let amount = "1234FIL";
        assert!(FILAmount::from_str(amount).is_ok());
    }

    #[test]
    fn invalid_fil_amount() {
        //bad amount fails
        let amount = "0.0.0FIL";
        assert!(FILAmount::from_str(amount).is_err());
    }

    #[test]
    fn test_fractional_fil_amount() {
        //fil with fractional value succeeds
        let amount = "1.234FIL";
        assert!(FILAmount::from_str(amount).is_ok());
    }

    #[test]
    fn fil_amount_too_long() {
        //fil amount with length>50 fails
        let amount = "100000000000000000000000000000000000000000000000000FIL";
        assert!(FILAmount::from_str(amount).is_err());
    }

    #[test]
    fn convert_fil_to_attofil() {
        //expected attofil amount matches actual amount after conversion from FIL
        let fil_amount = "1FIL";
        let attofil_amount = TokenAmount::from_atto(BigInt::from(FILECOIN_PRECISION));
        assert_eq!(
            FILAmount::from_str(fil_amount).unwrap().value,
            attofil_amount
        );
    }

    #[test]
    fn invalid_fil_suffix() {
        //test with bad suffix
        let amount = "42fiascos";
        assert!(FILAmount::from_str(amount).is_err());
    }

    #[quickcheck]
    fn fil_quickcheck_test() {
        todo!();
    }
}

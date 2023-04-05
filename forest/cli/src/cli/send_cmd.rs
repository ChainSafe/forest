// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use forest_json::message::json::MessageJson;
use forest_rpc_client::{mpool_push_message, wallet_default_address};
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, METHOD_SEND};
use num::BigInt;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;

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
        let suffix = suffix.replace(' ', "");

        let mut multiplier = dec!(1.0);
        let prefix = if !suffix.is_empty() {
            match suffix.trim().to_lowercase().strip_suffix("fil") {
                Some("atto" | "a") => "atto",
                Some("femto") => {
                    multiplier *= dec!(1_000);
                    "femto"
                }
                Some("pico") => {
                    multiplier *= dec!(1_000_000);
                    "pico"
                }
                Some("nano") => {
                    multiplier *= dec!(1_000_000_000);
                    "nano"
                }
                Some("micro") => {
                    multiplier *= dec!(1_000_000_000_000);
                    "micro"
                }
                Some("milli") => {
                    multiplier *= dec!(1_000_000_000_000_000);
                    "milli"
                }
                Some("" | " ") => {
                    multiplier *= dec!(1_000_000_000_000_000_000);
                    ""
                }
                _ => {
                    return Err(anyhow::anyhow!("unrecognized suffix: {}", suffix));
                }
            }
        } else {
            multiplier *= dec!(1_000_000_000_000_000_000);
            ""
        };

        if val.chars().count() > 50 {
            return Err(anyhow::anyhow!(
                "string length too large: {}",
                val.chars().count()
            ));
        }

        let parsed_val = match Decimal::from_str(val) {
            Ok(value) => value,
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "failed to parse {} as a decimal number",
                    val
                ))
            }
        };

        let attofil_val = if (parsed_val * multiplier).fract() != dec!(0.0) {
            return Err(anyhow::anyhow!("invalid {}FIL value: {}", prefix, val));
        } else {
            (parsed_val * multiplier).trunc().to_u128()
        };

        let token_amount = match attofil_val {
            Some(attofil_amt) => TokenAmount::from_atto(BigInt::from(attofil_amt)),
            None => return Err(anyhow::anyhow!("invalid {}FIL value: {}", prefix, val)),
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

    #[test]
    fn negative_fil_value() {
        //test with bad suffix
        let amount = "-1FIL";
        assert!(FILAmount::from_str(amount).is_err());
    }

    #[quickcheck]
    fn fil_quickcheck_test(n: u64) {
        let token_amount = TokenAmount::from_atto(n);
        let formatted =
            format_balance_string(token_amount.clone(), bool_pair_to_mode(true, false)).unwrap();
        let parsed = FILAmount::from_str(&formatted).unwrap().value;
        assert_eq!(token_amount, parsed);
    }
}

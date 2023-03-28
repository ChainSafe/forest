// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use forest_json::message::json::MessageJson;
use forest_rpc_client::{mpool_push_message, wallet_default_address};
use fraction::BigFraction;
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, METHOD_SEND};
use lazy_static::lazy_static;
use num::{bigint::Sign, BigInt, BigUint, CheckedMul};
use quickcheck_macros::quickcheck;
use regex::Regex;

use super::{handle_rpc_err, Config};

lazy_static! {
    static ref FIL_REG: Regex = Regex::new(r"^(?:\d*\.)?\d+").unwrap();
}

const FILECOIN_PRECISION: u64 = 1_000_000_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FILAmount {
    value: TokenAmount,
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

        let mut r = match BigFraction::from_str(val) {
            Ok(r) => r,
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "failed to parse {} as a decimal number",
                    val
                ))
            }
        };

        if !is_attofil {
            r = BigFraction::checked_mul(
                &r,
                &BigFraction::new(BigUint::from(FILECOIN_PRECISION), BigUint::from(1_u64)),
            )
            .unwrap();
        }

        if r.numer().unwrap() % r.denom().unwrap() != BigUint::from(0_u64) {
            let mut prefix = "";
            if is_attofil {
                prefix = "atto";
            }
            return Err(anyhow::anyhow!("invalid {}FIL value: {}", prefix, val));
        }

        Ok(FILAmount {
            value: TokenAmount::from_atto(BigInt::from_biguint(
                Sign::Plus,
                r.numer().unwrap() / r.denom().unwrap(),
            )),
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

#[test]
fn invalid_attofil_amount() {
    //attoFIL with fractional value fails (fractional FIL values allowed)
    let amount = "1.234attofil";
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

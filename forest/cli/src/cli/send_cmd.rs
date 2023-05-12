// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use forest_json::message::json::MessageJson;
use forest_rpc_client::{mpool_push_message, wallet_default_address};
use fvm_ipld_encoding::Cbor;
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, METHOD_SEND};
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
            value: self.amount.clone(),
            method_num: METHOD_SEND,
            gas_limit: self.gas_limit,
            gas_fee_cap: self.gas_feecap.clone(),
            gas_premium: self.gas_premium.clone(),
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

#[cfg(never)]
mod tests {
    // Generates a `TokenAmount` from a positive 64-bit integer and formats this
    // `TokenAmount` in two modes (`ExactFixed` and `ExactNotFixed`) using the
    // `format_balance_string` function, then parses these two output strings with
    // `FILAmount::from_str` and checks that the roundtrip results are the same
    // as the input `TokenAmount`
    fn fil_quickcheck_test(n: u64) {
        let token_amount = TokenAmount::from_atto(n);
        let formatted_not_fixed =
            format_balance_string(token_amount.clone(), FormattingMode::ExactNotFixed).unwrap();
        let formatted_fixed =
            format_balance_string(token_amount.clone(), FormattingMode::ExactFixed).unwrap();
        let parsed_not_fixed = FILAmount::from_str(&formatted_not_fixed).unwrap().value;
        let parsed_fixed = FILAmount::from_str(&formatted_fixed).unwrap().value;
        assert!(token_amount == parsed_not_fixed && token_amount == parsed_fixed);
    }

    // We want the `fil_quickcheck_test` to take an integer with an equal
    // probability of being any of the possible `FIL` units. However, because the
    // probability of `quickcheck` randomly generating an integer in the `attoFIL`
    // range is very low, while the probability of generating an integer in the
    // `FIL` range is very high, we need to flatten this skewed distribution
    // equally across the different unit types. This scaling function generates two
    // uniform distributions: the first is composed of random, positive 64-bit
    // integers and the second is composed of random, positive 32-bit integers.
    // In each instance, this function uses the second value to cap the digits
    // of the first value at 19 digits or fewer.
    #[quickcheck]
    fn scaled_fil_quickcheck_test(n: u64, rand_num: u32) {
        let scaled_n = n % u64::pow(10, rand_num % 20);
        fil_quickcheck_test(scaled_n);
    }

    #[quickcheck]
    fn fil_random_string_quickcheck_test(random_string: String) {
        let _ = FILAmount::from_str(&random_string);
    }
}

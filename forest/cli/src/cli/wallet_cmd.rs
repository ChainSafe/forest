// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Config;
use ahash::HashMap;
use ahash::HashMapExt;
use anyhow::Context;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use forest_json::address::json::AddressJson;
use forest_json::signature::json::{signature_type::SignatureTypeJson, SignatureJson};
use forest_key_management::json::KeyInfoJson;
use forest_rpc_client::wallet_ops::*;
use forest_utils::io::read_file_to_string;
use fvm_shared::address::{Address, Protocol};
use fvm_shared::bigint::BigInt;
use fvm_shared::crypto::signature::{Signature, SignatureType};
use fvm_shared::econ::TokenAmount;
use lazy_static::lazy_static;
use regex::Regex;
use rpassword::read_password;
use std::{
    path::PathBuf,
    str::{self, FromStr},
};
use structopt::StructOpt;

lazy_static! {
    static ref LEN_TO_CLOSURE_POWERS: HashMap<usize, (String, u8)> = {
        let mut map = HashMap::new();
        for item in 0..4 {
            map.insert(item, ("atto FIL".to_string(), 18));
        }
        for item in 4..7 {
            map.insert(item, ("femto FIL".to_string(), 15));
        }
        for item in 7..10 {
            map.insert(item, ("pico FIL".to_string(), 12));
        }
        for item in 10..13 {
            map.insert(item, ("nano FIL".to_string(), 9));
        }
        for item in 13..16 {
            map.insert(item, ("micro FIL".to_string(), 6));
        }
        for item in 16..19 {
            map.insert(item, ("milli FIL".to_string(), 3));
        }
        map
    };
}

use super::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    /// Create a new wallet
    New {
        /// The signature type to use. One of SECP256k1, or BLS
        #[structopt(default_value = "secp256k1")]
        signature_type: String,
    },
    /// Get account balance
    Balance {
        /// The address of the account to check
        address: String,
    },
    /// Get the default address of the wallet
    Default,
    /// Export the wallet's keys
    Export {
        /// The address that contains the keys to export
        address: String,
    },
    /// Check if the wallet has a key
    Has {
        /// The key to check
        key: String,
    },
    /// Import keys from existing wallet
    Import {
        /// The path to the private key
        path: Option<String>,
    },
    /// List addresses of the wallet
    List {
        /// flag to force full accuracy,
        /// not just default 4 significant digits
        /// E.g. 500.2367798 milli FIL instead of 500.2367 milli FIL
        /// In compination with `--fixed-unit` flag
        /// it will show exact data in `FIL` units
        /// E.g. 0.0000002367798 FIL instead of 0 FIL
        #[structopt(short, long)]
        exact_balance: bool,
        /// flag to force the balance to be in `FIL`
        /// meaning one won't balance in `atto` or `micro`
        /// form even if it is appropriate
        /// E.g. 0.5002 FIL instead of 500.2367 milli FIL
        /// In compination with `--exact-balance` flag
        /// it will show exact data in `FIL` units
        /// E.g. 0.0000002367798 FIL instead of 0 FIL
        #[structopt(short, long)]
        fixed_unit: bool,
    },
    /// Set the default wallet address
    SetDefault {
        /// The given key to set to the default address
        key: String,
    },
    /// Sign a message
    Sign {
        /// The hex encoded message to sign
        #[structopt(short)]
        message: String,
        /// The address to be used to sign the message
        #[structopt(short)]
        address: String,
    },
    /// Verify the signature of a message. Returns true if the signature matches the message and address
    Verify {
        /// The address used to sign the message
        #[structopt(short)]
        address: String,
        /// The message to verify
        #[structopt(short)]
        message: String,
        /// The signature of the message to verify
        #[structopt(short)]
        signature: String,
    },
}

impl WalletCommands {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::New { signature_type } => {
                let signature_type = match signature_type.to_lowercase().as_str() {
                    "secp256k1" => SignatureType::Secp256k1,
                    _ => SignatureType::BLS,
                };

                let signature_type_json = SignatureTypeJson(signature_type);

                let response = wallet_new((signature_type_json,), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                println!("{response}");
                Ok(())
            }
            Self::Balance { address } => {
                let response = wallet_balance((address.to_string(),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                println!("{response}");
                Ok(())
            }
            Self::Default => {
                let response = wallet_default_address(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                println!("{response}");
                Ok(())
            }
            Self::Export { address } => {
                let response = wallet_export((address.to_string(),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let encoded_key = serde_json::to_string(&response)?;
                println!("{}", hex::encode(encoded_key));
                Ok(())
            }
            Self::Has { key } => {
                let response = wallet_has((key.to_string(),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                println!("{response}");
                Ok(())
            }
            Self::Import { path } => {
                let key = match path {
                    Some(path) => read_file_to_string(&PathBuf::from(path))?,
                    _ => {
                        println!("Enter the private key: ");
                        read_password()?
                    }
                };

                let key = key.trim();

                let decoded_key = hex::decode(key).context("Key must be hex encoded")?;

                let key_str = str::from_utf8(&decoded_key)?;

                let key: KeyInfoJson =
                    serde_json::from_str(key_str).context("invalid key format")?;

                let key = wallet_import(vec![KeyInfoJson(key.0)], &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                println!("{key}");
                Ok(())
            }
            Self::List {
                exact_balance,
                fixed_unit,
            } => {
                let response = wallet_list(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let default = wallet_default_address(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let (title_address, title_default_mark, title_balance) =
                    ("Address", "Default", "Balance");
                println!("{title_address:41} {title_default_mark:7} {title_balance}");

                for address in response {
                    let addr = address.0.to_string();
                    let default_address_mark = if addr == default { "X" } else { "" };

                    let balance_string = wallet_balance((addr.clone(),), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;

                    let balance_int = TokenAmount::from_atto(balance_string.parse::<BigInt>()?);

                    let formatted_balance_string =
                        format_balance_string(balance_int, fixed_unit, exact_balance);

                    println!("{addr:41}  {default_address_mark:7}  {formatted_balance_string}",);
                }
                Ok(())
            }
            Self::SetDefault { key } => {
                let key =
                    Address::from_str(key).with_context(|| format!("Invalid address: {key}"))?;

                let key_json = AddressJson(key);
                wallet_set_default((key_json,), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                Ok(())
            }
            Self::Sign { address, message } => {
                let address = Address::from_str(address)
                    .with_context(|| format!("Invalid address: {address}"))?;

                let message = hex::decode(message).context("Message has to be a hex string")?;
                let message = BASE64_STANDARD.encode(message);

                let response = wallet_sign(
                    (AddressJson(address), message.into_bytes()),
                    &config.client.rpc_token,
                )
                .await
                .map_err(handle_rpc_err)?;
                println!("{}", hex::encode(response.0.bytes()));
                Ok(())
            }
            Self::Verify {
                message,
                address,
                signature,
            } => {
                let sig_bytes =
                    hex::decode(signature).context("Signature has to be a hex string")?;
                let address = Address::from_str(address)
                    .with_context(|| format!("Invalid address: {address}"))?;
                let signature = match address.protocol() {
                    Protocol::Secp256k1 => Signature::new_secp256k1(sig_bytes),
                    Protocol::BLS => Signature::new_bls(sig_bytes),
                    _ => anyhow::bail!("Invalid signature (must be bls or secp256k1)"),
                };
                let msg = hex::decode(message).context("Message has to be a hex string")?;

                let response = wallet_verify(
                    (AddressJson(address), msg, SignatureJson(signature)),
                    &config.client.rpc_token,
                )
                .await
                .map_err(handle_rpc_err)?;

                println!("{response}");
                Ok(())
            }
        }
    }
}

fn formating_vars(balance_string: String, balance_exact: String) -> (String, String) {
    //unfallible unwrap
    let re = Regex::new(r"(?i)(0+$)|(\.0+$)|(\.+$)").unwrap();
    let res = re.replace(balance_string.as_str(), "").to_string();
    let balance_exact_res = re.replace(balance_exact.as_str(), "").to_string();
    let symbol = if balance_exact_res.len() <= res.len() {
        ""
    } else {
        "~"
    };
    (res, symbol.to_string())
}

fn format_balance_string(
    mut balance_int: TokenAmount,
    fixed_unit: &bool,
    exact_balance: &bool,
) -> String {
    let mut unit = "FIL";
    let (balance_string, symbol) = if *fixed_unit {
        if *exact_balance {
            formating_vars(format!("{balance_int}"), format!("{balance_int}"))
        } else {
            formating_vars(format!("{balance_int:.0}0"), format!("{balance_int}"))
        }
    } else {
        let atto = balance_int.atto();
        let len = atto.to_string().len();
        if len <= 18 {
            //unfallible unwrap
            let (unit_string, closure_power) = LEN_TO_CLOSURE_POWERS.get(&len).unwrap();
            unit = unit_string.as_str();
            balance_int *= BigInt::from(10i64.pow(*closure_power as u32));
        }
        if *exact_balance {
            formating_vars(format!("{balance_int}"), format!("{balance_int}"))
        } else {
            formating_vars(format!("{balance_int:.4}"), format!("{balance_int}"))
        }
    };
    format!("{symbol}{balance_string} {unit}")
}

#[test]
fn exact_balance_fixed_unit() {
    assert_eq!(
        format_balance_string(TokenAmount::from_atto(100), &true, &true,),
        "0.0000000000000001 FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(12465), &true, &true,),
        "0.000000000000012465 FIL"
    );
}

#[test]
fn not_exact_balance_fixed_unit() {
    assert_eq!(
        format_balance_string(TokenAmount::from_atto(100), &true, &false,),
        "~0 FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(1000005000), &true, &false,),
        "~0 FIL"
    );

    assert_eq!(
        format_balance_string(
            TokenAmount::from_atto(15089000000000050000u64),
            &true,
            &false,
        ),
        "~15 FIL"
    );
}

#[test]
fn exact_balance_not_fixed_unit() {
    assert_eq!(
        format_balance_string(TokenAmount::from_atto(100), &false, &true,),
        "100 atto FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(120005), &false, &true,),
        "120.005 femto FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(200000045i64), &false, &true,),
        "200.000045 pico FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(1000000123), &false, &true,),
        "1.000000123 nano FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(450000008000000i64), &false, &true,),
        "450.000008 micro FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(90000002750000000i64), &false, &true,),
        "90.00000275 milli FIL"
    );
}

#[test]
fn not_exact_balance_not_fixed_unit() {
    assert_eq!(
        format_balance_string(TokenAmount::from_atto(100), &false, &false,),
        "100 atto FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(120005), &false, &false,),
        "120.005 femto FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(200000045i64), &false, &false,),
        "~200 pico FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(1000000123), &false, &false,),
        "~1 nano FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(450000008000000i64), &false, &false,),
        "~450 micro FIL"
    );

    assert_eq!(
        format_balance_string(TokenAmount::from_atto(90000002750000000i64), &false, &false,),
        "~90 milli FIL"
    );
}

#[test]
fn test_formatting_vars() {
    assert_eq!(
        formating_vars("1940.00".to_string(), "1940.000".to_string()),
        ("1940".to_string(), "".to_string())
    );
    assert_eq!(
        formating_vars("940.050".to_string(), "940.050123".to_string()),
        ("940.05".to_string(), "~".to_string())
    );
    assert_eq!(
        formating_vars("230.".to_string(), "230.0".to_string()),
        ("230".to_string(), "".to_string())
    );
}

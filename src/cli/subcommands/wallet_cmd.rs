// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    path::PathBuf,
    str::{self, FromStr},
};

use crate::json::{
    address::json::AddressJson,
    signature::json::{signature_type::SignatureTypeJson, SignatureJson},
};
use crate::key_management::json::KeyInfoJson;
use crate::rpc_client::wallet_ops::*;
use crate::shim::{
    address::{Address, Protocol, StrictAddress},
    crypto::{Signature, SignatureType},
    econ::TokenAmount,
};
use crate::utils::io::read_file_to_string;
use anyhow::Context;
use base64::{prelude::BASE64_STANDARD, Engine};
use clap::{arg, Subcommand};
use dialoguer::{theme::ColorfulTheme, Password};
use num::BigInt;

use super::{handle_rpc_err, Config};
use crate::cli::humantoken::TokenAmountPretty as _;

#[derive(Debug, Subcommand)]
pub enum WalletCommands {
    /// Create a new wallet
    New {
        /// The signature type to use. One of SECP256k1, or BLS
        #[arg(default_value = "secp256k1")]
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
        /// Output is rounded to 4 significant figures by default.
        /// Do not round
        // ENHANCE(aatifsyed): add a --round/--no-round argument pair
        #[arg(long, alias = "exact-balance", short_alias = 'e')]
        no_round: bool,
        /// Output may be given an SI prefix like `atto` by default.
        /// Do not do this, showing whole FIL at all times.
        #[arg(long, alias = "fixed-unit", short_alias = 'f')]
        no_abbrev: bool,
    },
    /// Set the default wallet address
    SetDefault {
        /// The given key to set to the default address
        key: String,
    },
    /// Sign a message
    Sign {
        /// The hex encoded message to sign
        #[arg(short)]
        message: String,
        /// The address to be used to sign the message
        #[arg(short)]
        address: String,
    },
    /// Verify the signature of a message. Returns true if the signature matches
    /// the message and address
    Verify {
        /// The address used to sign the message
        #[arg(short)]
        address: String,
        /// The message to verify
        #[arg(short)]
        message: String,
        /// The signature of the message to verify
        #[arg(short)]
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
                let response = wallet_default_address((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?
                    .unwrap_or_else(|| "No default wallet address set".to_string());
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
                        tokio::task::spawn_blocking(|| {
                            Password::with_theme(&ColorfulTheme::default())
                                .allow_empty_password(true)
                                .with_prompt("Enter the private key")
                                .interact()
                        })
                        .await??
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
                no_round,
                no_abbrev,
            } => {
                let response = wallet_list((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let default = wallet_default_address((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let (title_address, title_default_mark, title_balance) =
                    ("Address", "Default", "Balance");
                println!("{title_address:41} {title_default_mark:7} {title_balance}");

                for address in response {
                    let addr = address.0.to_string();
                    let default_address_mark = if default.as_ref() == Some(&addr) {
                        "X"
                    } else {
                        ""
                    };

                    let balance_string = wallet_balance((addr.clone(),), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;

                    let balance_token_amount =
                        TokenAmount::from_atto(balance_string.parse::<BigInt>()?);

                    let balance_string = match (no_round, no_abbrev) {
                        // no_round, absolute
                        (true, true) => format!("{:#}", balance_token_amount.pretty()),
                        // no_round, relative
                        (true, false) => format!("{}", balance_token_amount.pretty()),
                        // round, absolute
                        (false, true) => format!("{:#.4}", balance_token_amount.pretty()),
                        // round, relative
                        (false, false) => format!("{:.4}", balance_token_amount.pretty()),
                    };

                    println!("{addr:41}  {default_address_mark:7}  {balance_string}");
                }
                Ok(())
            }
            Self::SetDefault { key } => {
                let StrictAddress(key) = StrictAddress::from_str(key)
                    .with_context(|| format!("Invalid address: {key}"))?;

                let key_json = AddressJson(key);
                wallet_set_default((key_json,), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                Ok(())
            }
            Self::Sign { address, message } => {
                let StrictAddress(address) = StrictAddress::from_str(address)
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
                let StrictAddress(address) = StrictAddress::from_str(address)
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

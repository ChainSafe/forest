// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    path::PathBuf,
    str::{self, FromStr},
};

use crate::{cli::humantoken, message::SignedMessage, shim::address::Address};
use crate::{key_management::Key, utils::io::read_file_to_string};
use crate::{key_management::KeyInfo, rpc_client::ApiInfo};
use crate::{lotus_json::LotusJson, KeyStore};
use crate::{
    shim::{
        address::{Protocol, StrictAddress},
        crypto::{Signature, SignatureType},
        econ::TokenAmount,
        message::{Message, METHOD_SEND},
    },
    KeyStoreConfig,
};
use anyhow::{bail, Context as _};
use base64::{prelude::BASE64_STANDARD, Engine};
use clap::{arg, Subcommand};
use dialoguer::{theme::ColorfulTheme, Password};
use directories::ProjectDirs;
use num::BigInt;
use num::Zero as _;

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
    /// Validates whether a given string can be decoded as a well-formed address
    ValidateAddress {
        /// The address to be validated
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
    /// Deletes the wallet associated with the given address.
    Delete {
        /// The address of the wallet to delete
        address: String,
    },
    /// Send funds between accounts
    Send {
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
    },
}
impl WalletCommands {
    pub async fn run(self, api: ApiInfo, remote_wallet: bool) -> anyhow::Result<()> {
        let local_keystore = if !remote_wallet {
            let Some(dir) = ProjectDirs::from("com", "ChainSafe", "Forest-Wallet") else {
                bail!("Failed to find wallet directory");
            };
            // FIXME: Support encrypted wallets
            let keystore = KeyStore::new(KeyStoreConfig::Persistent(dir.data_dir().to_path_buf()))?;
            Some(keystore)
        } else {
            None
        };
        match self {
            Self::New { signature_type } => {
                let signature_type = match signature_type.to_lowercase().as_str() {
                    "secp256k1" => SignatureType::Secp256k1,
                    _ => SignatureType::Bls,
                };

                let addr = if let Some(mut keystore) = local_keystore {
                    let key = crate::key_management::generate_key(signature_type)?;

                    let addr = format!("wallet-{}", key.address);
                    keystore.put(&addr, key.key_info.clone())?;
                    let value = keystore.get("default");
                    if value.is_err() {
                        keystore.put("default", key.key_info)?
                    }

                    key.address.to_string()
                } else {
                    api.wallet_new(signature_type).await?
                };
                println!("{addr}");
                Ok(())
            }
            Self::Balance { address } => {
                let balance = api.wallet_balance(address.to_string()).await?;
                println!("{balance}");
                Ok(())
            }
            Self::Default => {
                let default_addr = if let Some(keystore) = local_keystore {
                    crate::key_management::get_default(&keystore)?.map(|s| s.to_string())
                } else {
                    api.wallet_default_address().await?
                }
                .context("No default wallet address set")?;
                println!("{default_addr}");
                Ok(())
            }
            Self::Export {
                address: address_string,
            } => {
                let StrictAddress(address) = StrictAddress::from_str(&address_string)
                    .with_context(|| format!("Invalid address: {address_string}"))?;

                let key_info = if let Some(keystore) = local_keystore {
                    crate::key_management::export_key_info(&address, &keystore)?
                } else {
                    api.wallet_export(address.to_string()).await?
                };

                let encoded_key = serde_json::to_string(&LotusJson(key_info))?;
                println!("{}", hex::encode(encoded_key));
                Ok(())
            }
            Self::Has { key } => {
                let StrictAddress(address) = StrictAddress::from_str(&key)
                    .with_context(|| format!("Invalid address: {key}"))?;

                let response = if let Some(keystore) = local_keystore {
                    crate::key_management::find_key(&address, &keystore).is_ok()
                } else {
                    api.wallet_has(address.to_string()).await?
                };
                println!("{response}");
                Ok(())
            }
            Self::Delete { address } => {
                let StrictAddress(address) = StrictAddress::from_str(&address)
                    .with_context(|| format!("Invalid address: {address}"))?;

                if let Some(mut keystore) = local_keystore {
                    crate::key_management::remove_key(&address, &mut keystore)?;
                } else {
                    api.wallet_delete(address.to_string()).await?;
                }
                println!("deleted {address}.");
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

                let LotusJson(key_info) = serde_json::from_str::<LotusJson<KeyInfo>>(key_str)
                    .context("invalid key format")?;

                let key = if let Some(mut keystore) = local_keystore {
                    let key = Key::try_from(key_info)?;
                    let addr = format!("wallet-{}", key.address);

                    keystore.put(&addr, key.key_info)?;
                    key.address.to_string()
                } else {
                    api.wallet_import(vec![key_info]).await?
                };

                println!("{key}");
                Ok(())
            }
            Self::List {
                no_round,
                no_abbrev,
            } => {
                let response = if let Some(keystore) = &local_keystore {
                    crate::key_management::list_addrs(keystore)?
                } else {
                    api.wallet_list().await?
                };

                let default = if let Some(keystore) = &local_keystore {
                    crate::key_management::get_default(keystore)?.map(|s| s.to_string())
                } else {
                    api.wallet_default_address().await?
                };

                let (title_address, title_default_mark, title_balance) =
                    ("Address", "Default", "Balance");
                println!("{title_address:41} {title_default_mark:7} {title_balance}");

                for address in response {
                    let addr = address.to_string();
                    let default_address_mark = if default.as_ref() == Some(&addr) {
                        "X"
                    } else {
                        ""
                    };

                    let balance_string = api.wallet_balance(addr.clone()).await?;

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
                let StrictAddress(key) = StrictAddress::from_str(&key)
                    .with_context(|| format!("Invalid address: {key}"))?;

                if let Some(mut keystore) = local_keystore {
                    let addr_string = format!("wallet-{}", key);
                    let key_info = keystore.get(&addr_string)?;
                    keystore.remove("default")?; // This line should unregister current default key then continue
                    keystore.put("default", key_info)?;
                } else {
                    api.wallet_set_default(key).await?;
                }
                Ok(())
            }
            Self::Sign { address, message } => {
                let StrictAddress(address) = StrictAddress::from_str(&address)
                    .with_context(|| format!("Invalid address: {address}"))?;

                let message = hex::decode(message).context("Message has to be a hex string")?;
                let message = BASE64_STANDARD.encode(message);

                let signature = if let Some(keystore) = local_keystore {
                    let key = crate::key_management::find_key(&address, &keystore)?;

                    crate::key_management::sign(
                        *key.key_info.key_type(),
                        key.key_info.private_key(),
                        &BASE64_STANDARD.decode(message)?,
                    )?
                } else {
                    api.wallet_sign(address, message.into_bytes()).await?
                };
                println!("{}", hex::encode(signature.bytes()));
                Ok(())
            }
            Self::ValidateAddress { address } => {
                let response = api.wallet_validate_address(address.to_string()).await?;
                println!("{response}");
                Ok(())
            }
            Self::Verify {
                message,
                address,
                signature,
            } => {
                let sig_bytes =
                    hex::decode(signature).context("Signature has to be a hex string")?;
                let StrictAddress(address) = StrictAddress::from_str(&address)
                    .with_context(|| format!("Invalid address: {address}"))?;
                let signature = match address.protocol() {
                    Protocol::Secp256k1 => Signature::new_secp256k1(sig_bytes),
                    Protocol::BLS => Signature::new_bls(sig_bytes),
                    _ => anyhow::bail!("Invalid signature (must be bls or secp256k1)"),
                };
                let msg = hex::decode(message).context("Message has to be a hex string")?;

                let response = if !remote_wallet {
                    signature.verify(&msg, &address).is_ok()
                } else {
                    // Relying on a remote server to validate signatures is not secure but it's useful for testing.
                    api.wallet_verify(address, msg, signature).await?
                };

                println!("{response}");
                Ok(())
            }
            Self::Send {
                from,
                target_address,
                amount,
                gas_feecap,
                gas_limit,
                gas_premium,
            } => {
                let from: Address = if let Some(from) = from {
                    StrictAddress::from_str(&from)?.into()
                } else {
                    StrictAddress::from_str(
                        &if let Some(keystore) = &local_keystore {
                            crate::key_management::get_default(keystore)?.map(|s| s.to_string())
                        } else {
                            api.wallet_default_address().await?
                        }
                        .context(
                            "No default wallet address selected. Please set a default address.",
                        )?,
                    )?
                    .into()
                };

                let message = Message {
                    from,
                    to: StrictAddress::from_str(&target_address)?.into(),
                    value: amount,
                    method_num: METHOD_SEND,
                    gas_limit: gas_limit as u64,
                    gas_fee_cap: gas_feecap,
                    gas_premium,
                    // JANK(aatifsyed): Why are we using a testing build of fvm_shared?
                    ..Default::default()
                };

                let signed_msg = if let Some(keystore) = local_keystore {
                    let spec = None;
                    let tsk = Default::default();
                    let mut message = api.gas_estimate_message_gas(message, spec, tsk).await?;

                    if message.gas_premium > message.gas_fee_cap {
                        anyhow::bail!("After estimation, gas premium is greater than gas fee cap")
                    }

                    message.sequence = api.mpool_get_nonce(from).await?;

                    let key = crate::key_management::find_key(&from, &keystore)?;
                    let sig = crate::key_management::sign(
                        *key.key_info.key_type(),
                        key.key_info.private_key(),
                        message.cid().unwrap().to_bytes().as_slice(),
                    )?;

                    let smsg = SignedMessage::new_from_parts(message, sig)?;
                    api.mpool_push(smsg.clone()).await?;
                    smsg
                } else {
                    api.mpool_push_message(message, None).await?
                };

                println!("{}", signed_msg.cid().unwrap());

                Ok(())
            }
        }
    }
}

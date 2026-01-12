// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    cell::RefCell,
    path::PathBuf,
    str::{self, FromStr},
};

use crate::cli::humantoken::TokenAmountPretty as _;
use crate::key_management::{Key, KeyInfo};
use crate::{
    ENCRYPTED_KEYSTORE_NAME,
    cli::humantoken,
    message::SignedMessage,
    rpc::{
        mpool::{MpoolGetNonce, MpoolPush, MpoolPushMessage},
        types::ApiTipsetKey,
    },
    shim::address::Address,
};
use crate::{KeyStore, lotus_json::LotusJson};
use crate::{
    KeyStoreConfig,
    shim::{
        address::StrictAddress,
        crypto::{Signature, SignatureType},
        econ::TokenAmount,
        message::{METHOD_SEND, Message},
    },
};
use crate::{
    lotus_json::HasLotusJson as _,
    rpc::{self, prelude::*},
};
use anyhow::{Context as _, bail};
use base64::{Engine, prelude::BASE64_STANDARD};
use clap::Subcommand;
use dialoguer::{Password, console::Term, theme::ColorfulTheme};
use directories::ProjectDirs;
use num::Zero as _;

// Abstraction over local and remote wallets. A connection to a running Filecoin
// node is always required for balance queries and for sending messages. When a
// local wallet is available, no sensitive information will be sent to the
// remote Filecoin node.
struct WalletBackend {
    pub remote: rpc::Client,
    pub local: Option<KeyStore>,
}

impl WalletBackend {
    fn new_remote(client: rpc::Client) -> Self {
        WalletBackend {
            remote: client,
            local: None,
        }
    }

    fn new_local(client: rpc::Client, want_encryption: bool) -> anyhow::Result<Self> {
        let Some(dir) = ProjectDirs::from("com", "ChainSafe", "Forest-Wallet") else {
            bail!("Failed to find wallet directory");
        };

        let wallet_dir = dir.data_dir().to_path_buf();

        let is_encrypted = wallet_dir.join(ENCRYPTED_KEYSTORE_NAME).exists();

        // Always use the encrypted keystore if it exists. It it does not exist,
        // only use encryption when explicitly asked for it.
        let keystore = if is_encrypted || want_encryption {
            input_password_to_load_encrypted_keystore(wallet_dir)?
        } else {
            KeyStore::new(KeyStoreConfig::Persistent(wallet_dir.to_path_buf()))?
        };

        Ok(WalletBackend {
            remote: client,
            local: Some(keystore),
        })
    }

    async fn list_addrs(&self) -> anyhow::Result<Vec<Address>> {
        if let Some(keystore) = &self.local {
            Ok(crate::key_management::list_addrs(keystore)?)
        } else {
            Ok(WalletList::call(&self.remote, ()).await?)
        }
    }

    async fn wallet_export(&self, address: Address) -> anyhow::Result<KeyInfo> {
        if let Some(keystore) = &self.local {
            Ok(crate::key_management::export_key_info(&address, keystore)?)
        } else {
            Ok(WalletExport::call(&self.remote, (address,)).await?)
        }
    }

    async fn wallet_import(&mut self, key_info: KeyInfo) -> anyhow::Result<String> {
        if let Some(keystore) = &mut self.local {
            let key = Key::try_from(key_info)?;
            let addr = format!("wallet-{}", key.address);

            keystore.put(&addr, key.key_info)?;
            Ok(key.address.to_string())
        } else {
            Ok(WalletImport::call(&self.remote, (key_info,))
                .await?
                .to_string())
        }
    }

    async fn wallet_has(&self, address: Address) -> anyhow::Result<bool> {
        if let Some(keystore) = &self.local {
            Ok(crate::key_management::find_key(&address, keystore).is_ok())
        } else {
            Ok(WalletHas::call(&self.remote, (address,)).await?)
        }
    }

    async fn wallet_delete(&mut self, address: Address) -> anyhow::Result<()> {
        if let Some(keystore) = &mut self.local {
            Ok(crate::key_management::remove_key(&address, keystore)?)
        } else {
            Ok(WalletDelete::call(&self.remote, (address,)).await?)
        }
    }

    async fn wallet_new(&mut self, signature_type: SignatureType) -> anyhow::Result<String> {
        if let Some(keystore) = &mut self.local {
            let key = crate::key_management::generate_key(signature_type)?;

            let addr = format!("wallet-{}", key.address);
            keystore.put(&addr, key.key_info.clone())?;
            let value = keystore.get("default");
            if value.is_err() {
                keystore.put("default", key.key_info)?
            }

            Ok(key.address.to_string())
        } else {
            Ok(WalletNew::call(&self.remote, (signature_type,))
                .await?
                .to_string())
        }
    }

    async fn wallet_default_address(&self) -> anyhow::Result<Option<String>> {
        if let Some(keystore) = &self.local {
            Ok(crate::key_management::get_default(keystore)?.map(|s| s.to_string()))
        } else {
            Ok(WalletDefaultAddress::call(&self.remote, ())
                .await?
                .map(|it| it.to_string()))
        }
    }

    async fn wallet_set_default(&mut self, address: Address) -> anyhow::Result<()> {
        if let Some(keystore) = &mut self.local {
            let addr_string = format!("wallet-{address}");
            let key_info = keystore.get(&addr_string)?;
            keystore.remove("default")?; // This line should unregister current default key then continue
            keystore.put("default", key_info)?;
            Ok(())
        } else {
            Ok(WalletSetDefault::call(&self.remote, (address,)).await?)
        }
    }

    async fn wallet_sign(&self, address: Address, message: String) -> anyhow::Result<Signature> {
        if let Some(keystore) = &self.local {
            let key = crate::key_management::find_key(&address, keystore)?;

            Ok(crate::key_management::sign(
                *key.key_info.key_type(),
                key.key_info.private_key(),
                &BASE64_STANDARD.decode(message)?,
            )?)
        } else {
            Ok(WalletSign::call(&self.remote, (address, message.into_bytes())).await?)
        }
    }

    async fn wallet_verify(
        &self,
        address: Address,
        msg: Vec<u8>,
        signature: Signature,
    ) -> anyhow::Result<bool> {
        if self.local.is_some() {
            Ok(signature.verify(&msg, &address).is_ok())
        } else {
            // Relying on a remote server to validate signatures is not secure but it's useful for testing.
            Ok(WalletVerify::call(&self.remote, (address, msg, signature)).await?)
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum WalletCommands {
    /// Create a new wallet
    New {
        /// The signature type to use. One of `secp256k1`, `bls` or `delegated`
        #[arg(default_value = "secp256k1")]
        signature_type: SignatureType,
    },
    /// Get account balance
    Balance {
        /// The address of the account to check
        address: String,
        /// Output is rounded to 4 significant figures by default.
        /// Do not round
        // ENHANCE(aatifsyed): add a --round/--no-round argument pair
        #[arg(long, alias = "exact-balance")]
        no_round: bool,
        /// Output may be given an SI prefix like `atto` by default.
        /// Do not do this, showing whole FIL at all times.
        #[arg(long, alias = "fixed-unit")]
        no_abbrev: bool,
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
        #[arg(long, alias = "exact-balance")]
        no_round: bool,
        /// Output may be given an SI prefix like `atto` by default.
        /// Do not do this, showing whole FIL at all times.
        #[arg(long, alias = "fixed-unit")]
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
    pub async fn run(
        self,
        client: rpc::Client,
        remote_wallet: bool,
        encrypt: bool,
    ) -> anyhow::Result<()> {
        let mut backend = if remote_wallet {
            WalletBackend::new_remote(client)
        } else {
            WalletBackend::new_local(client, encrypt)?
        };
        match self {
            Self::New { signature_type } => {
                let addr: String = backend.wallet_new(signature_type).await?;
                println!("{addr}");
                Ok(())
            }
            Self::Balance {
                address,
                no_round,
                no_abbrev,
            } => {
                let StrictAddress(address) = StrictAddress::from_str(&address)
                    .with_context(|| format!("Invalid address: {address}"))?;
                let balance = WalletBalance::call(&backend.remote, (address,)).await?;
                println!("{}", format_balance(&balance, no_round, no_abbrev));
                Ok(())
            }
            Self::Default => {
                let default_addr = backend
                    .wallet_default_address()
                    .await?
                    .context("No default wallet address set")?;
                println!("{default_addr}");
                Ok(())
            }
            Self::Export {
                address: address_string,
            } => {
                let StrictAddress(address) = StrictAddress::from_str(&address_string)
                    .with_context(|| format!("Invalid address: {address_string}"))?;
                let key_info = backend.wallet_export(address).await?;
                let encoded_key = key_info.into_lotus_json_string()?;
                println!("{}", hex::encode(encoded_key));
                Ok(())
            }
            Self::Has { key } => {
                let StrictAddress(address) = StrictAddress::from_str(&key)
                    .with_context(|| format!("Invalid address: {key}"))?;

                println!("{response}", response = backend.wallet_has(address).await?);
                Ok(())
            }
            Self::Delete { address } => {
                let StrictAddress(address) = StrictAddress::from_str(&address)
                    .with_context(|| format!("Invalid address: {address}"))?;

                backend.wallet_delete(address).await?;
                println!("deleted {address}.");
                Ok(())
            }
            Self::Import { path } => {
                let key = match path {
                    Some(path) => std::fs::read_to_string(path)?,
                    _ => {
                        let term = Term::stderr();
                        if term.is_term() {
                            tokio::task::spawn_blocking(|| {
                                Password::with_theme(&ColorfulTheme::default())
                                    .allow_empty_password(true)
                                    .with_prompt("Enter the private key")
                                    .interact()
                            })
                            .await??
                        } else {
                            let mut buffer = String::new();
                            std::io::stdin().read_line(&mut buffer)?;
                            buffer
                        }
                    }
                };

                let key = key.trim();

                let decoded_key = hex::decode(key).context("Key must be hex encoded")?;

                let key_str = str::from_utf8(&decoded_key)?;

                let LotusJson(key_info) = serde_json::from_str::<LotusJson<KeyInfo>>(key_str)
                    .context("invalid key format")?;

                let key = backend.wallet_import(key_info).await?;

                println!("{key}");
                Ok(())
            }
            Self::List {
                no_round,
                no_abbrev,
            } => {
                let key_pairs = backend.list_addrs().await?;
                let default = backend.wallet_default_address().await?;

                let max_addr_len = key_pairs
                    .iter()
                    .map(|addr| addr.to_string().len())
                    .max()
                    .unwrap_or(42);

                println!(
                    "{:<width_addr$} {:<width_default$} Balance",
                    "Address",
                    "Default",
                    width_addr = max_addr_len,
                    width_default = 7,
                );

                for address in key_pairs {
                    let default_address_mark = if default.as_ref() == Some(&address.to_string()) {
                        "X"
                    } else {
                        ""
                    };

                    let balance_token_amount =
                        WalletBalance::call(&backend.remote, (address,)).await?;

                    let balance_string = format_balance(&balance_token_amount, no_round, no_abbrev);

                    println!(
                        "{:<width_addr$} {:<width_default$} {}",
                        address.to_string(),
                        default_address_mark,
                        balance_string,
                        width_addr = max_addr_len,
                        width_default = 7,
                    );
                }
                Ok(())
            }
            Self::SetDefault { key } => {
                let StrictAddress(key) = StrictAddress::from_str(&key)
                    .with_context(|| format!("Invalid address: {key}"))?;

                backend.wallet_set_default(key).await
            }
            Self::Sign { address, message } => {
                let StrictAddress(address) = StrictAddress::from_str(&address)
                    .with_context(|| format!("Invalid address: {address}"))?;

                let message = hex::decode(message).context("Message has to be a hex string")?;
                let message = BASE64_STANDARD.encode(message);

                let signature = backend.wallet_sign(address, message).await?;
                println!("{}", hex::encode(signature.to_bytes()));
                Ok(())
            }
            Self::ValidateAddress { address } => {
                let response = WalletValidateAddress::call(&backend.remote, (address,)).await?;
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
                let msg = hex::decode(message).context("Message has to be a hex string")?;

                let signature = Signature::from_bytes(sig_bytes)?;
                let is_valid = backend.wallet_verify(address, msg, signature).await?;

                println!("{is_valid}");
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
                    StrictAddress::from_str(&backend.wallet_default_address().await?.context(
                        "No default wallet address selected. Please set a default address.",
                    )?)?
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
                    ..Default::default()
                };

                let signed_msg = if let Some(keystore) = &backend.local {
                    let spec = None;
                    let mut message = GasEstimateMessageGas::call(
                        &backend.remote,
                        (message, spec, ApiTipsetKey(None)),
                    )
                    .await?
                    .message;

                    if message.gas_premium > message.gas_fee_cap {
                        anyhow::bail!("After estimation, gas premium is greater than gas fee cap")
                    }

                    message.sequence = MpoolGetNonce::call(&backend.remote, (from,)).await?;

                    let key = crate::key_management::find_key(&from, keystore)?;
                    let sig = crate::key_management::sign(
                        *key.key_info.key_type(),
                        key.key_info.private_key(),
                        message.cid().to_bytes().as_slice(),
                    )?;

                    let smsg = SignedMessage::new_from_parts(message, sig)?;

                    MpoolPush::call(&backend.remote, (smsg.clone(),)).await?;
                    smsg
                } else {
                    MpoolPushMessage::call(&backend.remote, (message, None)).await?
                };

                println!("{}", signed_msg.cid());

                Ok(())
            }
        }
    }
}

/// Prompts for password, looping until the [`KeyStore`] is successfully loaded.
///
/// This code makes blocking syscalls.
fn input_password_to_load_encrypted_keystore(data_dir: PathBuf) -> dialoguer::Result<KeyStore> {
    let keystore = RefCell::new(None);
    let term = Term::stderr();

    // Unlike `dialoguer::Confirm`, `dialoguer::Password` doesn't fail if the terminal is not a tty
    // so do that check ourselves.
    // This means users can't pipe their password from stdin.
    if !term.is_term() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "cannot read password from non-terminal",
        )
        .into());
    }

    dialoguer::Password::new()
        .with_prompt("Enter the password for the wallet keystore")
        .allow_empty_password(true) // let validator do validation
        .validate_with(|input: &String| {
            KeyStore::new(KeyStoreConfig::Encrypted(data_dir.clone(), input.clone()))
                .map(|created| *keystore.borrow_mut() = Some(created))
                .context(
                    "Error: couldn't load keystore with this password. Try again or press Ctrl+C to abort.",
                )
        })
        .interact_on(&term)?;

    Ok(keystore
        .into_inner()
        .expect("validation succeeded, so keystore must be emplaced"))
}

fn format_balance(balance: &TokenAmount, no_round: bool, no_abbrev: bool) -> String {
    match (no_round, no_abbrev) {
        // no_round, absolute
        (true, true) => format!("{:#}", balance.pretty()),
        // no_round, relative
        (true, false) => format!("{}", balance.pretty()),
        // round, absolute
        (false, true) => format!("{:#.4}", balance.pretty()),
        // round, relative
        (false, false) => format!("{:.4}", balance.pretty()),
    }
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    cell::RefCell,
    path::PathBuf,
    str::{self, FromStr},
    time::{Duration, Instant},
};

use crate::cli::humantoken::TokenAmountPretty as _;
use crate::key_management::{Key, KeyInfo};
use crate::{
    ENCRYPTED_KEYSTORE_NAME,
    cli::humantoken,
    eth::{EAMMethod, EVMMethod},
    rpc::{
        eth::{EthChainId, is_eth_address, types::EthAddress},
        mpool::{MpoolGetNonce, MpoolPush, MpoolPushMessage},
        types::ApiTipsetKey,
    },
    shim::{
        address::{Address, Protocol},
        message::{METHOD_SEND, Message},
    },
};
use crate::{
    KeyStore, KeyStoreConfig,
    lotus_json::{HasLotusJson as _, LotusJson},
    rpc::{self, prelude::*},
    shim::{
        address::StrictAddress,
        crypto::{Signature, SignatureType},
        econ::TokenAmount,
    },
};
use anyhow::{Context as _, bail};
use clap::Subcommand;
use dialoguer::{Password, console::Term, theme::ColorfulTheme};
use directories::ProjectDirs;
use jsonrpsee::core::ClientError;
use num::Zero as _;
use tabled::{builder::Builder, settings::Style};

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

    fn new_local(client: rpc::Client, want_encryption: Option<bool>) -> anyhow::Result<Self> {
        let Some(dir) = ProjectDirs::from("com", "ChainSafe", "Forest-Wallet") else {
            bail!("Failed to find wallet directory");
        };
        let wallet_dir = dir.data_dir().to_path_buf();
        let is_encrypted = wallet_dir.join(ENCRYPTED_KEYSTORE_NAME).exists();
        // Default to an encrypted keystore if it exist.
        let use_encryption = want_encryption.unwrap_or(is_encrypted);
        let keystore = if use_encryption {
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
            Ok(crate::key_management::try_find_key(&address, keystore).is_ok())
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

    async fn wallet_default_address(&self) -> anyhow::Result<Option<Address>> {
        if let Some(keystore) = &self.local {
            Ok(crate::key_management::get_default(keystore)?)
        } else {
            Ok(WalletDefaultAddress::call(&self.remote, ()).await?)
        }
    }

    async fn wallet_set_default(&mut self, address: Address) -> anyhow::Result<()> {
        if let Some(keystore) = &mut self.local {
            let key_info = crate::key_management::try_find(&address, keystore)?;
            keystore.set_default(key_info)?;
            Ok(())
        } else {
            Ok(WalletSetDefault::call(&self.remote, (address,)).await?)
        }
    }

    async fn wallet_sign(&self, address: Address, message: Vec<u8>) -> anyhow::Result<Signature> {
        if let Some(keystore) = &self.local {
            let key = crate::key_management::try_find_key(&address, keystore)?;

            Ok(crate::key_management::sign(
                *key.key_info.key_type(),
                key.key_info.private_key(),
                &message,
            )?)
        } else {
            Ok(WalletSign::call(&self.remote, (address, message)).await?)
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
        address: StrictAddress,
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
        address: StrictAddress,
    },
    /// Check if the wallet has a key
    Has {
        /// The key to check
        key: StrictAddress,
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
        key: StrictAddress,
    },
    /// Sign a message
    Sign {
        /// The hex encoded message to sign
        #[arg(short)]
        message: String,
        /// The address to be used to sign the message
        #[arg(short)]
        address: StrictAddress,
        /// Sign the raw message bytes without the FRC-0102 envelope. Use this
        /// for interoperating with pre-FRC-0102 tooling, or when the bytes are
        /// already an on-chain Filecoin message (which must not be wrapped).
        #[arg(long)]
        raw: bool,
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
        address: StrictAddress,
        /// The message to verify
        #[arg(short)]
        message: String,
        /// The signature of the message to verify
        #[arg(short)]
        signature: String,
        /// Verify against the raw message bytes without applying the
        /// FRC-0102 envelope. Use this for signatures produced by
        /// pre-FRC-0102 tooling or for on-chain Filecoin messages (which are
        /// signed raw, without the envelope).
        #[arg(long)]
        raw: bool,
    },
    /// Deletes the wallet associated with the given address.
    Delete {
        /// The address of the wallet to delete
        address: StrictAddress,
    },
    /// Send funds between accounts
    Send {
        /// optionally specify the account to send funds from (otherwise the default
        /// one will be used)
        #[arg(long)]
        from: Option<StrictAddress>,
        /// The recipient address. Accepts either a FIL address (e.g.
        /// `f1.../t1...`) or an ETH address (e.g. `0x...`).
        // Kept as `String` rather than `StrictAddress` because the latter
        // rejects the ETH form, which `resolve_target_address` handles.
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
        /// Wait for the message to be on chain with the given confidence by calling `StateWaitMsg`.
        /// The command waits until the message has been on chain for at least `confidence` epochs.
        #[arg(long)]
        wait_confidence: Option<u32>,
        /// Timeout duration for `--wait-confidence`, e.g. `30s`, `5m`. If not set, the timeout will be `confidence + 5` epochs.
        #[arg(long, requires = "wait_confidence", value_parser = humantime::parse_duration)]
        wait_timeout: Option<Duration>,
    },
}
impl WalletCommands {
    pub async fn run(
        self,
        client: rpc::Client,
        remote_wallet: bool,
        encrypt: Option<bool>,
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
                let balance = WalletBalance::call(&backend.remote, (address.into(),)).await?;
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
            Self::Export { address } => {
                let key_info = backend.wallet_export(address.into()).await?;
                let encoded_key = key_info.into_lotus_json_string()?;
                println!("{}", hex::encode(encoded_key));
                Ok(())
            }
            Self::Has { key } => {
                println!(
                    "{response}",
                    response = backend.wallet_has(key.into()).await?
                );
                Ok(())
            }
            Self::Delete { address } => {
                let address: Address = address.into();
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
                let (key_pairs, default_address) =
                    tokio::try_join!(backend.list_addrs(), backend.wallet_default_address(),)?;

                let remote = &backend.remote;
                let results =
                    futures::future::join_all(key_pairs.iter().copied().map(|a| async move {
                        let result = StateGetActor::call(remote, (a, ApiTipsetKey(None))).await;
                        (a, result)
                    }))
                    .await;
                let mut rows: Vec<_> = results
                    .into_iter()
                    .map(|(a, result)| {
                        if let Err(e) = &result {
                            tracing::warn!(%a, %e, "failed to get actor state for wallet list");
                        }
                        let actor = result.ok().flatten();
                        let balance: TokenAmount = actor
                            .as_ref()
                            .map(|s| s.balance.clone().into())
                            .unwrap_or_default();
                        let nonce = actor.as_ref().map(|s| s.sequence).unwrap_or_default();
                        (a, balance, nonce)
                    })
                    .collect();
                rows.sort_by_key(|(a, _, _)| default_address != Some(*a));

                let mut builder = Builder::default();
                builder.push_record(["Address", "Balance", "Nonce"]);
                for (addr, balance, nonce) in &rows {
                    let addr_str = if default_address == Some(*addr) {
                        format!("{addr} (default)")
                    } else {
                        addr.to_string()
                    };
                    let balance = format_balance(balance, no_round, no_abbrev);
                    builder.push_record([&addr_str, &balance, &nonce.to_string()]);
                }

                let mut list = builder.build();
                list.with(Style::blank());
                println!("{list}");
                Ok(())
            }
            Self::SetDefault { key } => backend.wallet_set_default(key.into()).await,
            Self::Sign {
                address,
                message,
                raw,
            } => {
                let message = hex::decode(message).context("Message has to be a hex string")?;
                let message = if raw { message } else { wrap_frc0102(&message) };

                let signature = backend.wallet_sign(address.into(), message).await?;
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
                raw,
            } => {
                let sig_bytes =
                    hex::decode(signature).context("Signature has to be a hex string")?;
                let msg = hex::decode(message).context("Message has to be a hex string")?;
                let msg = if raw { msg } else { wrap_frc0102(&msg) };

                let signature = Signature::from_bytes(sig_bytes)?;
                let is_valid = backend
                    .wallet_verify(address.into(), msg, signature)
                    .await?;

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
                wait_confidence,
                wait_timeout,
            } => {
                let from: Address = match from {
                    Some(a) => a.into(),
                    None => backend.wallet_default_address().await?.context(
                        "No default wallet address selected. Please set a default address.",
                    )?,
                };

                let (mut to, is_0x_recipient) = resolve_target_address(&target_address)?;

                // Resolve to ID address when sending from delegated address to non-ID/non-Delegated address.
                if is_eth_address(&from)
                    && to.protocol() != Protocol::ID
                    && to.protocol() != Protocol::Delegated
                {
                    to = StateLookupID::call(&backend.remote, (to, ApiTipsetKey(None)))
                        .await
                        .with_context(|| {
                            format!(
                                "addresses starting with f410f can only send to other addresses starting with f410f, or id addresses. could not find id address for {to}"
                            )
                        })?;
                }
                let method_num = resolve_method_num(&from, &to, is_0x_recipient);

                let message = Message {
                    from,
                    to,
                    value: amount,
                    method_num,
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
                    .await?;

                    if message.gas_premium > message.gas_fee_cap {
                        anyhow::bail!("After estimation, gas premium is greater than gas fee cap")
                    }

                    message.sequence = MpoolGetNonce::call(&backend.remote, (from,)).await?;

                    let key = crate::key_management::try_find_key(&from, keystore)?;
                    let eth_chain_id = u64::from_str_radix(
                        EthChainId::call(&backend.remote, ())
                            .await?
                            .trim_start_matches("0x"),
                        16,
                    )?;
                    let smsg = crate::key_management::sign_message(&key, &message, eth_chain_id)?;
                    MpoolPush::call(&backend.remote, (smsg.clone(),)).await?;
                    smsg
                } else {
                    MpoolPushMessage::call(&backend.remote, (message, None)).await?
                };

                let msg_cid = signed_msg.cid();
                println!("{msg_cid}");

                if let Some(confidence) = wait_confidence {
                    let start = Instant::now();
                    let version = Version::call(&backend.remote, ()).await?;
                    let timeout = wait_timeout.unwrap_or_else(|| {
                        Duration::from_secs(u64::from((confidence + 5) * version.block_delay))
                    });
                    backend
                        .remote
                        .call(
                            StateWaitMsg::request((
                                msg_cid,
                                i64::from(confidence),
                                10,
                                true,
                            ))?
                            .with_timeout(timeout),
                        )
                        .await
                        .map_err(|e|
                        {
                            match e {
                                ClientError::RequestTimeout => {
                                    anyhow::anyhow!("timed out waiting for the message {msg_cid} with confidence {confidence}, took {}", humantime::format_duration(start.elapsed()))
                                }
                                e => {
                                    anyhow::anyhow!("failed to wait for the message {msg_cid} with confidence {confidence}: {e}")
                                }
                            }
                        })?;
                }

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

fn resolve_target_address(target_address: &str) -> anyhow::Result<(Address, bool)> {
    match StrictAddress::from_str(target_address) {
        Ok(addr) => Ok((addr.into(), false)),
        Err(_) => {
            let eth_addr = EthAddress::from_str(target_address)
                .context("target address must be a valid FIL address or ETH address (0x...)")?;
            let addr = eth_addr.to_filecoin_address()?;
            Ok((addr, true))
        }
    }
}

const FRC_0102_FILECOIN_PREFIX: &[u8] = b"\x19Filecoin Signed Message:\n";

/// Wraps `msg` with the FRC-0102 envelope: `0x19 || "Filecoin Signed Message:\n" || ascii(len(msg)) || msg`
// See <https://github.com/filecoin-project/FIPs/blob/bdd5283279fd115c87c9bbf71d2e40c9d075f5aa/FRCs/frc-0102.md>.
fn wrap_frc0102(msg: &[u8]) -> Vec<u8> {
    let len = msg.len().to_string();
    [FRC_0102_FILECOIN_PREFIX, len.as_bytes(), msg].concat()
}

fn resolve_method_num(from: &Address, to: &Address, is_0x_recipient: bool) -> u64 {
    if !is_eth_address(from) && !is_0x_recipient {
        return METHOD_SEND;
    }
    if *to == Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR {
        EAMMethod::CreateExternal as u64
    } else {
        EVMMethod::InvokeContract as u64
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::eth::{EAMMethod, EVMMethod};
    use crate::rpc::eth::types::EthAddress;
    use crate::shim::address::{Address, CurrentNetwork, Network};
    use crate::shim::message::METHOD_SEND;
    use rstest::rstest;

    use super::{SignatureType, resolve_method_num, resolve_target_address, wrap_frc0102};

    #[test]
    fn test_resolve_target_address_id() {
        CurrentNetwork::with(Network::Mainnet, || {
            let (addr, is_0x) = resolve_target_address("f01234").unwrap();
            assert!(!is_0x);
            let expected_addr = Address::new_id(1234);
            assert_eq!(addr, expected_addr);
        });
        CurrentNetwork::with(Network::Testnet, || {
            let (addr, is_0x) = resolve_target_address("t01234").unwrap();
            assert!(!is_0x);
            let expected_addr = Address::new_id(1234);
            assert_eq!(addr, expected_addr);
        });
    }

    #[test]
    fn test_resolve_target_address_masked_id() {
        CurrentNetwork::with(Network::Mainnet, || {
            let (addr, is_0x) =
                resolve_target_address("0xff000000000000000000000000000000000004d2").unwrap();
            assert!(is_0x);
            let expected_addr = Address::new_id(1234);
            assert_eq!(addr, expected_addr);
        });
        CurrentNetwork::with(Network::Testnet, || {
            let (addr, is_0x) =
                resolve_target_address("0xff000000000000000000000000000000000004d2").unwrap();
            assert!(is_0x);
            let expected_addr = Address::new_id(1234);
            assert_eq!(addr, expected_addr);
        });
    }

    #[test]
    fn test_resolve_target_address_eth() {
        CurrentNetwork::with(Network::Mainnet, || {
            let (addr, is_0x) =
                resolve_target_address("0x6cb414224f0b91de5c3b616e700e34a5172c149f").unwrap();
            assert!(is_0x);
            let expected_addr =
                Address::from_str("f410fns2biispboi54xb3mfxhadruuulsyfe73avfmey").unwrap();
            assert_eq!(addr, expected_addr);
        });
        CurrentNetwork::with(Network::Testnet, || {
            let (addr, is_0x) =
                resolve_target_address("0x6cb414224f0b91de5c3b616e700e34a5172c149f").unwrap();
            assert!(is_0x);
            let expected_addr =
                Address::from_str("t410fns2biispboi54xb3mfxhadruuulsyfe73avfmey").unwrap();
            assert_eq!(addr, expected_addr);
        });
    }

    #[test]
    fn test_resolve_target_address_invalid() {
        let err = resolve_target_address("0xInvalidAddress").unwrap_err();
        assert!(
            err.to_string()
                .contains("target address must be a valid FIL address or ETH address")
        );
    }

    #[test]
    fn test_resolve_method_num_send() {
        let from = Address::from_str("f01234").unwrap();
        let to = Address::from_str("f01234").unwrap();
        let method = resolve_method_num(&from, &to, false);
        assert_eq!(method, METHOD_SEND);
    }

    #[test]
    fn test_resolve_method_num_create_external() {
        let from = Address::from_str("f410fvfpyxvy6aqet3g2bfbj6h7nr5kjgyncpaeimgxa").unwrap();
        let to = Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR;
        let method = resolve_method_num(&from, &to, false);
        assert_eq!(method, EAMMethod::CreateExternal as u64);
    }

    #[test]
    fn test_resolve_method_num_invoke_contract() {
        let from = Address::from_str("f410fvfpyxvy6aqet3g2bfbj6h7nr5kjgyncpaeimgxa").unwrap();
        let to = Address::from_str("f410fvfpyxvy6aqet3g2bfbj6h7nr5kjgyncpaeimgxa").unwrap();
        let method = resolve_method_num(&from, &to, false);
        assert_eq!(method, EVMMethod::InvokeContract as u64);
    }

    #[test]
    fn test_resolve_method_num_invoke_contract_eth() {
        let from = Address::from_str("f410fvfpyxvy6aqet3g2bfbj6h7nr5kjgyncpaeimgxa").unwrap();
        let to = EthAddress::from_str("0x6cb414224f0b91de5c3b616e700e34a5172c149f")
            .unwrap()
            .to_filecoin_address()
            .unwrap();
        let method = resolve_method_num(&from, &to, true);
        assert_eq!(method, EVMMethod::InvokeContract as u64);
    }

    #[test]
    fn test_resolve_method_num_send_to_delegated() {
        let from = Address::from_str("f01234").unwrap();
        let to = Address::from_str("f410fvfpyxvy6aqet3g2bfbj6h7nr5kjgyncpaeimgxa").unwrap();
        let method = resolve_method_num(&from, &to, false);
        assert_eq!(method, METHOD_SEND);
    }

    #[test]
    fn test_resolve_method_num_send_to_eth() {
        let from = Address::from_str("f01234").unwrap();
        let to = EthAddress::from_str("0x6cb414224f0b91de5c3b616e700e34a5172c149f")
            .unwrap()
            .to_filecoin_address()
            .unwrap();
        let method = resolve_method_num(&from, &to, true);
        assert_eq!(method, EVMMethod::InvokeContract as u64);
    }

    #[rstest]
    #[case::empty(&[])]
    #[case::short(b"hello")]
    #[case::longer(b"this is a longer test message")]
    // Non-UTF-8 bytes must pass through unchanged.
    #[case::binary(&[0x00, 0xFF, 0x10, 0x80])]
    // The spec does not require any escaping; embedded newlines pass through.
    #[case::newline(b"line1\nline2")]
    fn test_wrap_frc0102(#[case] msg: &[u8]) {
        // The envelope is always `0x19 || "Filecoin Signed Message:\n" || ascii(len) || msg`.
        let mut expected = b"\x19Filecoin Signed Message:\n".to_vec();
        expected.extend_from_slice(msg.len().to_string().as_bytes());
        expected.extend_from_slice(msg);
        assert_eq!(wrap_frc0102(msg), expected);
    }

    #[rstest]
    // Length is encoded as decimal ASCII; check 1-, 2-, 3- and 4-digit boundaries.
    #[case(0)]
    #[case(9)]
    #[case(10)]
    #[case(99)]
    #[case(100)]
    #[case(999)]
    #[case(1000)]
    fn test_wrap_frc0102_length_boundaries(#[case] len: usize) {
        let msg = vec![0xABu8; len];
        let wrapped = wrap_frc0102(&msg);
        let digits = len.to_string();
        assert_eq!(&wrapped[..26], b"\x19Filecoin Signed Message:\n");
        assert_eq!(&wrapped[26..26 + digits.len()], digits.as_bytes());
        assert_eq!(&wrapped[26 + digits.len()..], msg.as_slice());
    }

    #[test]
    fn test_frc0102_roundtrip_sign_verify_secp256k1() {
        use crate::key_management::generate_key;
        let key = generate_key(SignatureType::Secp256k1).unwrap();
        let raw_msg = b"hello world";
        let wrapped = wrap_frc0102(raw_msg);

        // Sign the wrapped bytes (what `forest-wallet sign` does by default).
        let signature = crate::key_management::sign(
            *key.key_info.key_type(),
            key.key_info.private_key(),
            &wrapped,
        )
        .unwrap();

        // `verify` on the wrapped bytes must succeed.
        signature.verify(&wrapped, &key.address).unwrap();

        assert!(
            signature.verify(raw_msg, &key.address).is_err(),
            "raw-bytes verify should fail when the signature was produced over the FRC-0102 envelope"
        );
    }
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Calibnet wallet integration tests for Forest.
//!
//! Daemon orchestration (snapshot import, node spawn, sync wait) is done by
//! the wrappers under `scripts/tests/` via `scripts/tests/harness.sh`; this
//! crate runs the wallet-specific test logic against the resulting daemon.
//!
//! The [`WalletHarness`] type wraps the in-process Forest RPC client and an
//! on-disk [`KeyStore`], mirroring the local/remote split used by the
//! `forest-wallet` binary. The [`scenarios`] module contains the test flows.

use std::future::Future;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context as _, anyhow, bail};
use cid::Cid;
use directories::ProjectDirs;
use num_traits::Zero;
use tracing::{info, warn};

use forest::ENCRYPTED_KEYSTORE_NAME;
use forest::KeyStore;
use forest::KeyStoreConfig;
use forest::interop_tests_private::eth::{EAMMethod, EVMMethod};
use forest::interop_tests_private::humantoken;
use forest::interop_tests_private::key_management::{Key, KeyInfo};
use forest::interop_tests_private::lotus_json::{HasLotusJson, LotusJson};
use forest::interop_tests_private::message::SignedMessage;
use forest::interop_tests_private::rpc::{
    self,
    eth::{is_eth_address, types::EthAddress},
    mpool::{MpoolGetNonce, MpoolPush, MpoolPushMessage},
    prelude::*,
    types::ApiTipsetKey,
};
use forest::interop_tests_private::shim::{
    address::{Address, Protocol, StrictAddress},
    crypto::SignatureType,
    econ::TokenAmount,
    message::{METHOD_SEND, Message},
};

pub mod scenarios;

/// Maximum number of polling attempts for balance updates before giving up.
pub const POLL_RETRIES: u32 = 20;

/// Duration between polling attempts.
pub const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Selects which wallet backend an operation targets, equivalent to the
/// presence or absence of `--remote-wallet` on the `forest-wallet` CLI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    /// File-system keystore at `~/.local/share/Forest-Wallet/`. Equivalent to
    /// `forest-wallet ...` (no `--remote-wallet`).
    Local,
    /// Daemon-hosted keystore reached via the JSON-RPC API. Equivalent to
    /// `forest-wallet --remote-wallet ...`.
    Remote,
}

/// Test harness wrapping a remote RPC client plus a local keystore.
///
/// Mirrors the local/remote split of `WalletBackend` in
/// `src/wallet/subcommands/wallet_cmd.rs`, kept minimal and focused on what
/// the test scenarios need.
pub struct WalletHarness {
    remote: rpc::Client,
    local: KeyStore,
}

impl WalletHarness {
    /// Construct a harness using `FULLNODE_API_INFO` for the RPC endpoint and
    /// the conventional non-encrypted keystore directory for local keys.
    pub fn from_env() -> anyhow::Result<Self> {
        let remote = rpc::Client::default_or_from_env(None)
            .context("failed to construct RPC client (is FULLNODE_API_INFO set?)")?;
        let local = open_default_keystore()?;
        Ok(Self { remote, local })
    }

    /// List addresses known to the selected backend.
    pub async fn list(&self, backend: Backend) -> anyhow::Result<Vec<Address>> {
        match backend {
            Backend::Local => Ok(forest::interop_tests_private::key_management::list_addrs(
                &self.local,
            )?),
            Backend::Remote => Ok(WalletList::call(&self.remote, ()).await?),
        }
    }

    /// Create a new address with the given signature type.
    pub async fn new_address(
        &mut self,
        backend: Backend,
        sig_type: SignatureType,
    ) -> anyhow::Result<Address> {
        match backend {
            Backend::Local => {
                let key = forest::interop_tests_private::key_management::generate_key(sig_type)?;
                let addr_key = format!("wallet-{}", key.address);
                self.local.put(&addr_key, key.key_info.clone())?;
                if self.local.get("default").is_err() {
                    self.local.put("default", key.key_info)?;
                }
                Ok(key.address)
            }
            Backend::Remote => Ok(WalletNew::call(&self.remote, (sig_type,)).await?),
        }
    }

    /// Import a key into the selected backend.
    pub async fn import(&mut self, backend: Backend, key_info: KeyInfo) -> anyhow::Result<Address> {
        match backend {
            Backend::Local => {
                let key = Key::try_from(key_info)?;
                let addr_key = format!("wallet-{}", key.address);
                self.local.put(&addr_key, key.key_info)?;
                Ok(key.address)
            }
            Backend::Remote => Ok(WalletImport::call(&self.remote, (key_info,)).await?),
        }
    }

    /// Export the [`KeyInfo`] for `address` from the selected backend.
    pub async fn export(&self, backend: Backend, address: Address) -> anyhow::Result<KeyInfo> {
        match backend {
            Backend::Local => Ok(
                forest::interop_tests_private::key_management::export_key_info(
                    &address,
                    &self.local,
                )?,
            ),
            Backend::Remote => Ok(WalletExport::call(&self.remote, (address,)).await?),
        }
    }

    /// Delete `address` from the selected backend.
    pub async fn delete(&mut self, backend: Backend, address: Address) -> anyhow::Result<()> {
        match backend {
            Backend::Local => {
                forest::interop_tests_private::key_management::remove_key(
                    &address,
                    &mut self.local,
                )?;
                Ok(())
            }
            Backend::Remote => {
                WalletDelete::call(&self.remote, (address,)).await?;
                Ok(())
            }
        }
    }

    /// Set `address` as the default in the selected backend.
    pub async fn set_default(&mut self, backend: Backend, address: Address) -> anyhow::Result<()> {
        match backend {
            Backend::Local => {
                let addr_key = format!("wallet-{address}");
                let key_info = self.local.get(&addr_key)?;
                // `remove` is intentional: matches `forest-wallet`'s behaviour
                // and ensures we replace any prior default rather than fail
                // due to a duplicate insert.
                self.local.remove("default")?;
                self.local.put("default", key_info)?;
                Ok(())
            }
            Backend::Remote => {
                WalletSetDefault::call(&self.remote, (address,)).await?;
                Ok(())
            }
        }
    }

    /// Default address for the selected backend, if any.
    pub async fn default_address(&self, backend: Backend) -> anyhow::Result<Option<Address>> {
        match backend {
            Backend::Local => Ok(forest::interop_tests_private::key_management::get_default(
                &self.local,
            )?),
            Backend::Remote => Ok(WalletDefaultAddress::call(&self.remote, ()).await?),
        }
    }

    /// Always-fresh balance lookup (always via RPC; the chain is the source of truth).
    pub async fn balance(&self, address: Address) -> anyhow::Result<TokenAmount> {
        Ok(WalletBalance::call(&self.remote, (address,)).await?)
    }

    /// Convert a Filecoin address to its Ethereum equivalent via RPC.
    pub async fn filecoin_to_eth(&self, address: Address) -> anyhow::Result<EthAddress> {
        Ok(FilecoinAddressToEthAddress::call(&self.remote, (address, None)).await?)
    }

    /// Send `amount` from the selected backend's default address to `to`.
    ///
    /// Mirrors the local/remote signing split in
    /// `WalletCommands::Send` in `src/wallet/subcommands/wallet_cmd.rs`:
    /// when a local key is available we sign in-process and push via
    /// `Filecoin.MpoolPush`; otherwise we delegate to
    /// `Filecoin.MpoolPushMessage`.
    pub async fn send(
        &mut self,
        backend: Backend,
        to: Address,
        amount: TokenAmount,
    ) -> anyhow::Result<Cid> {
        let from = self
            .default_address(backend)
            .await?
            .context("no default address set on selected backend")?;

        let (mut to_resolved, is_0x_recipient) = (to, false);
        // Resolve to ID address when sending from a delegated address to a
        // non-ID/non-Delegated address, for parity with
        // `wallet_cmd::resolve_target_address`.
        if is_eth_address(&from)
            && to_resolved.protocol() != Protocol::ID
            && to_resolved.protocol() != Protocol::Delegated
        {
            to_resolved = StateLookupID::call(&self.remote, (to_resolved, ApiTipsetKey(None)))
                .await
                .with_context(|| {
                    format!(
                        "addresses starting with f410f can only send to other f410f or ID addresses, but no ID address was found for {to_resolved}"
                    )
                })?;
        }

        let method_num = resolve_method_num(&from, &to_resolved, is_0x_recipient);

        let message = Message {
            from,
            to: to_resolved,
            value: amount,
            method_num,
            gas_limit: 0,
            gas_fee_cap: TokenAmount::zero(),
            gas_premium: TokenAmount::zero(),
            ..Default::default()
        };

        let signed: SignedMessage = match backend {
            Backend::Local => {
                let mut estimated =
                    GasEstimateMessageGas::call(&self.remote, (message, None, ApiTipsetKey(None)))
                        .await?
                        .message;

                if estimated.gas_premium > estimated.gas_fee_cap {
                    bail!("after estimation, gas premium is greater than gas fee cap");
                }

                estimated.sequence = MpoolGetNonce::call(&self.remote, (from,)).await?;

                let key = forest::interop_tests_private::key_management::try_find_key(
                    &from,
                    &self.local,
                )?;
                let eth_chain_id = u64::from_str_radix(
                    EthChainId::call(&self.remote, ())
                        .await?
                        .trim_start_matches("0x"),
                    16,
                )?;
                let smsg = forest::interop_tests_private::key_management::sign_message(
                    &key,
                    &estimated,
                    eth_chain_id,
                )?;
                MpoolPush::call(&self.remote, (smsg.clone(),)).await?;
                smsg
            }
            Backend::Remote => MpoolPushMessage::call(&self.remote, (message, None)).await?,
        };

        Ok(signed.cid())
    }

    /// Send `amount` from the selected backend's default address to an
    /// Ethereum-format address (`0x…`). Equivalent to passing a `0x…`
    /// recipient to `forest-wallet send`.
    pub async fn send_to_eth(
        &mut self,
        backend: Backend,
        to: EthAddress,
        amount: TokenAmount,
    ) -> anyhow::Result<Cid> {
        let fil_target = to
            .to_filecoin_address()
            .context("could not convert ETH address to Filecoin address")?;
        let from = self
            .default_address(backend)
            .await?
            .context("no default address set on selected backend")?;

        let mut to_resolved = fil_target;
        if is_eth_address(&from)
            && to_resolved.protocol() != Protocol::ID
            && to_resolved.protocol() != Protocol::Delegated
        {
            to_resolved = StateLookupID::call(&self.remote, (to_resolved, ApiTipsetKey(None)))
                .await
                .with_context(|| {
                    format!(
                        "addresses starting with f410f can only send to other f410f or ID addresses, but no ID address was found for {to_resolved}"
                    )
                })?;
        }
        let method_num = resolve_method_num(&from, &to_resolved, /* is_0x_recipient */ true);

        let message = Message {
            from,
            to: to_resolved,
            value: amount,
            method_num,
            gas_limit: 0,
            gas_fee_cap: TokenAmount::zero(),
            gas_premium: TokenAmount::zero(),
            ..Default::default()
        };

        let signed: SignedMessage = match backend {
            Backend::Local => {
                let mut estimated =
                    GasEstimateMessageGas::call(&self.remote, (message, None, ApiTipsetKey(None)))
                        .await?
                        .message;
                if estimated.gas_premium > estimated.gas_fee_cap {
                    bail!("after estimation, gas premium is greater than gas fee cap");
                }
                estimated.sequence = MpoolGetNonce::call(&self.remote, (from,)).await?;

                let key = forest::interop_tests_private::key_management::try_find_key(
                    &from,
                    &self.local,
                )?;
                let eth_chain_id = u64::from_str_radix(
                    EthChainId::call(&self.remote, ())
                        .await?
                        .trim_start_matches("0x"),
                    16,
                )?;
                let smsg = forest::interop_tests_private::key_management::sign_message(
                    &key,
                    &estimated,
                    eth_chain_id,
                )?;
                MpoolPush::call(&self.remote, (smsg.clone(),)).await?;
                smsg
            }
            Backend::Remote => MpoolPushMessage::call(&self.remote, (message, None)).await?,
        };
        Ok(signed.cid())
    }
}

/// Polling combinator: invoke `f` up to `retries` times, sleeping `interval`
/// between attempts. Returns the first `Ok(Some(t))`. `Ok(None)` triggers
/// another poll; an `Err` is logged and also triggers another poll so transient
/// RPC failures don't abort the whole scenario.
pub async fn wait_until<F, Fut, T>(
    label: &str,
    retries: u32,
    interval: Duration,
    mut f: F,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<Option<T>>>,
{
    for attempt in 1..=retries {
        info!("{label}: attempt {attempt}/{retries}");
        match f().await {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(err) => {
                warn!("{label}: attempt {attempt}/{retries} errored: {err:#}");
            }
        }
        if attempt < retries {
            tokio::time::sleep(interval).await;
        }
    }
    Err(anyhow!(
        "{label}: timed out after {retries} attempts (interval {:?})",
        interval
    ))
}

/// Wait until `address` reports a balance strictly greater than `baseline`.
pub async fn wait_balance_above(
    harness: &WalletHarness,
    address: Address,
    baseline: TokenAmount,
    label: &str,
) -> anyhow::Result<TokenAmount> {
    wait_until(label, POLL_RETRIES, POLL_INTERVAL, || async {
        let current = harness.balance(address).await?;
        if current > baseline {
            Ok(Some(current))
        } else {
            Ok(None)
        }
    })
    .await
}

/// Wait until `address` reports a non-zero balance.
pub async fn wait_balance_nonzero(
    harness: &WalletHarness,
    address: Address,
    label: &str,
) -> anyhow::Result<TokenAmount> {
    wait_balance_above(harness, address, TokenAmount::zero(), label).await
}

/// Parse a human-readable token amount, e.g. `"500 atto FIL"` or `"3 micro FIL"`.
pub fn parse_amount(input: &str) -> anyhow::Result<TokenAmount> {
    humantoken::parse(input).with_context(|| format!("invalid token amount: {input:?}"))
}

/// Parse a Filecoin address from a string, with the same strictness as the
/// `forest-wallet` CLI (rejects loose forms).
pub fn parse_address(input: &str) -> anyhow::Result<Address> {
    let StrictAddress(addr) = StrictAddress::from_str(input)
        .with_context(|| format!("invalid Filecoin address: {input}"))?;
    Ok(addr)
}

/// Parse a `KeyInfo` from the hex-encoded JSON representation produced by
/// `forest-wallet export`.
pub fn parse_exported_key(hex_blob: &str) -> anyhow::Result<KeyInfo> {
    let json_bytes =
        hex::decode(hex_blob.trim()).context("exported key must be a hex-encoded string")?;
    let json_str = std::str::from_utf8(&json_bytes).context("exported key is not valid UTF-8")?;
    let LotusJson(key_info) = serde_json::from_str::<LotusJson<KeyInfo>>(json_str)
        .context("exported key has an invalid Lotus JSON shape")?;
    Ok(key_info)
}

/// Encode a `KeyInfo` in the same hex-of-Lotus-JSON format used by
/// `forest-wallet export`.
pub fn encode_exported_key(key_info: &KeyInfo) -> anyhow::Result<String> {
    let json = key_info.clone().into_lotus_json_string()?;
    Ok(hex::encode(json))
}

/// Open the persistent (non-encrypted) wallet keystore at the conventional
/// `directories::ProjectDirs` path. Returns an error if the keystore is
/// encrypted, since the test harness has no way to prompt for a password.
fn open_default_keystore() -> anyhow::Result<KeyStore> {
    let dir = ProjectDirs::from("com", "ChainSafe", "Forest-Wallet")
        .context("could not resolve Forest-Wallet project dirs")?;
    let wallet_dir: PathBuf = dir.data_dir().to_path_buf();

    if wallet_dir.join(ENCRYPTED_KEYSTORE_NAME).exists() {
        bail!(
            "encrypted keystore detected at {} — the test harness only supports the non-encrypted keystore",
            wallet_dir.display()
        );
    }

    Ok(KeyStore::new(KeyStoreConfig::Persistent(wallet_dir))?)
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

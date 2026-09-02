// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::eth::EAMMethod;
use crate::rpc::eth::{
    BlockNumberOrHash, Predefined,
    types::{EthAddress, EthBytes, EthCallMessage},
};
use crate::rpc::{self, prelude::*};
use crate::shim::actors::eam;
use crate::shim::address::Address;
use crate::shim::message::Message;
use crate::utils::encoding::{from_slice_with_fallback, hex};
use anyhow::Context as _;
use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use clap::Subcommand;
use fil_actor_eam_state::v16::CreateExternalParams;
use fvm_ipld_encoding::RawBytes;
use std::path::PathBuf;
use std::str::FromStr as _;

const WAIT_LOOKBACK: i64 = 800;

/// Commands related to the Filecoin EVM runtime
#[derive(Debug, Subcommand)]
pub enum EvmCommands {
    /// Deploy an EVM smart contract and return its address
    Deploy {
        /// Optionally specify the account to use for sending the creation message
        #[arg(long)]
        from: Option<Address>,
        /// Use when input contract is in hex
        #[arg(long)]
        hex: bool,
        /// Wait for message execution before returning (default: true)
        #[arg(long)]
        wait: Option<bool>,
        /// Contract init code
        contract: PathBuf,
    },
    /// Simulate an eth contract call
    Call {
        /// Ethereum sender address
        from: EthAddress,
        /// Ethereum contract address
        to: EthAddress,
        /// Hex-encoded call params
        params: EthBytes,
    },
}

impl EvmCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Deploy {
                from,
                hex,
                wait,
                contract,
            } => deploy(client, from, hex, wait.unwrap_or(true), contract).await,
            Self::Call { from, to, params } => call(client, from, to, params).await,
        }
    }
}

async fn deploy(
    client: rpc::Client,
    from: Option<Address>,
    is_hex: bool,
    wait: bool,
    contract: PathBuf,
) -> anyhow::Result<()> {
    let mut initcode = std::fs::read(&contract).context("failed to read contract")?;
    if is_hex {
        initcode = decode_hex_contract(&initcode).context("failed to decode contract")?;
    }

    let from = match from {
        Some(addr) => addr,
        None => WalletDefaultAddress::call(&client, ())
            .await?
            .context("no default wallet address")?,
    };

    let params = RawBytes::serialize(CreateExternalParams(initcode))
        .context("failed to serialize Create params")?;
    let msg = Message {
        to: Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR,
        from,
        method_num: EAMMethod::CreateExternal as u64,
        params,
        ..Default::default()
    };

    println!("sending message...");
    let smsg = MpoolPushMessage::call(&client, (msg, None))
        .await
        .context("failed to push message")?;
    let cid = smsg.cid();
    println!("Message CID: {cid}");

    if !wait {
        return Ok(());
    }

    println!("waiting for message to execute...");
    let lookup = StateWaitMsg::call(&client, (cid, 0, WAIT_LOOKBACK, true))
        .await
        .context("error waiting for message")?;

    println!("Exit Code: {}", lookup.receipt.exit_code().value());
    println!("Gas Used: {}", lookup.receipt.gas_used());

    anyhow::ensure!(
        lookup.receipt.exit_code().is_success(),
        "actor execution failed"
    );

    let return_bytes = lookup.receipt.return_data();
    let result: eam::CreateExternalReturn =
        from_slice_with_fallback(return_bytes.bytes()).context("error decoding return value")?;

    let id_addr = Address::new_id(result.actor_id);
    let eth = EthAddress::from(result.eth_address.0);
    let f4 = eth
        .to_filecoin_address()
        .context("failed to calculate f4 address")?;

    println!("Actor ID: {}", result.actor_id);
    println!("ID Address: {id_addr}");
    println!(
        "Robust Address: {}",
        result
            .robust_address
            .map(|a| Address::from(a).to_string())
            .unwrap_or_default()
    );
    println!("Eth Address: {}", hex::encode_prefixed(eth.0.as_bytes()));
    println!("f4 Address: {f4}");
    if !return_bytes.bytes().is_empty() {
        println!("Return: {}", BASE64_STANDARD.encode(return_bytes.bytes()));
    }

    Ok(())
}

async fn call(
    client: rpc::Client,
    from: EthAddress,
    to: EthAddress,
    params: EthBytes,
) -> anyhow::Result<()> {
    let result = EthCall::call(
        &client,
        (
            EthCallMessage {
                from: Some(from),
                to: Some(to),
                data: Some(params),
                ..Default::default()
            },
            BlockNumberOrHash::PredefinedBlock(Predefined::Latest),
        ),
    )
    .await;

    match result {
        Ok(res) => {
            println!("Result: {}", hex::encode_prefixed(&res.0));
            Ok(())
        }
        Err(e) => {
            println!("Eth call fails, return val: 0x");
            Err(e.into())
        }
    }
}

fn decode_hex_contract(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    let s = std::str::from_utf8(raw)?.trim();
    let s = s.strip_prefix("0X").unwrap_or(s);
    Ok(EthBytes::from_str(s)?.0)
}

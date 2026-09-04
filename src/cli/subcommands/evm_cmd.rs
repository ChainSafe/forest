// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::eth::{EAMMethod, EVMMethod};
use crate::rpc::eth::{
    BlockNumberOrHash, Predefined,
    types::{EthAddress, EthBytes, EthCallMessage},
};
use crate::rpc::types::MessageLookup;
use crate::rpc::{self, prelude::*};
use crate::shim::actors::eam;
use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;
use crate::shim::message::Message;
use crate::utils::encoding::{from_slice_with_fallback, hex};
use anyhow::Context as _;
use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use cid::Cid;
use clap::Subcommand;
use fil_actor_eam_state::v16::CreateExternalParams;
use fil_actor_evm_state::v16::{InvokeContractParams, InvokeContractReturn};
use fvm_ipld_encoding::RawBytes;
use std::path::PathBuf;
use std::str::FromStr as _;
use std::time::Duration;

const WAIT_LOOKBACK: i64 = 800;
const WAIT_TIMEOUT: Duration = Duration::from_mins(10);

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
    /// Invoke an EVM smart contract using the specified calldata
    Invoke {
        /// Optionally specify the account to use for sending the exec message
        #[arg(long)]
        from: Option<Address>,
        /// Value to send with the invocation message, in attoFIL
        #[arg(long, default_value_t = 0)]
        value: u64,
        /// Filecoin address of the contract
        address: Address,
        /// Hex-encoded ABI calldata
        calldata: EthBytes,
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
            Self::Invoke {
                from,
                value,
                address,
                calldata,
            } => invoke(client, from, value, address, calldata).await,
            Self::Call { from, to, params } => call(client, from, to, params).await,
        }
    }
}

async fn resolve_from(client: &rpc::Client, from: Option<Address>) -> anyhow::Result<Address> {
    match from {
        Some(addr) => Ok(addr),
        None => WalletDefaultAddress::call(client, ())
            .await?
            .context("no default wallet address"),
    }
}

async fn wait_for_message(client: &rpc::Client, cid: Cid) -> anyhow::Result<MessageLookup> {
    println!("waiting for message to execute...");
    client
        .call(StateWaitMsg::request((cid, 0, WAIT_LOOKBACK, true))?.with_timeout(WAIT_TIMEOUT))
        .await
        .context("error waiting for message")
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
        initcode = EthBytes::from_str(std::str::from_utf8(&initcode)?)
            .context("failed to decode contract")?
            .0;
    }

    let from = resolve_from(&client, from).await?;

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

    let lookup = wait_for_message(&client, cid).await?;

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

async fn invoke(
    client: rpc::Client,
    from: Option<Address>,
    value: u64,
    address: Address,
    calldata: EthBytes,
) -> anyhow::Result<()> {
    let from = resolve_from(&client, from).await?;
    let params = RawBytes::serialize(InvokeContractParams {
        input_data: calldata.0,
    })
    .context("failed to encode evm params as cbor")?;

    let msg = Message {
        to: address,
        from,
        value: TokenAmount::from_atto(value),
        method_num: EVMMethod::InvokeContract as u64,
        params,
        ..Default::default()
    };

    println!("sending message...");
    let smsg = MpoolPushMessage::call(&client, (msg, None))
        .await
        .context("failed to push message")?;
    let cid = smsg.cid();
    println!("Message CID: {cid}");

    let lookup = wait_for_message(&client, cid).await?;

    anyhow::ensure!(
        lookup.receipt.exit_code().is_success(),
        "actor execution failed"
    );

    println!("Gas used: {}", lookup.receipt.gas_used());

    let ret: InvokeContractReturn = from_slice_with_fallback(lookup.receipt.return_data().bytes())
        .context("evm result not correctly encoded")?;
    if ret.output_data.is_empty() {
        println!("OK");
    } else {
        println!("{}", hex::encode(&ret.output_data));
    }

    if let Some(root) = lookup.receipt.events_root() {
        let events = ChainGetEvents::call(&client, (root,))
            .await
            .context("failed to load events")?;
        println!("Events emitted:");
        for event in events {
            println!("\tEmitter ID: {}", event.emitter);
            for entry in event.entries {
                println!(
                    "\t\tKey: {}, Value: 0x{}, Flags: b{:b}",
                    entry.key,
                    hex::encode(&entry.value.0),
                    entry.flags
                );
            }
        }
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

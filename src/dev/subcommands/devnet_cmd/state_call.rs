// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! `Filecoin.StateCall` gas limit parity tests against the Lotus node on the docker devnet. Every
//! call runs on both nodes, which must agree.

use crate::dev::subcommands::tests_cmd::helpers::*;
use crate::rpc::eth::types::{EthAddress, EthBytes, EthCallMessage};
use crate::rpc::prelude::*;
use crate::rpc::state::ApiInvocResult;
use crate::rpc::types::ApiTipsetKey;
use crate::rpc::{Client, Request};
use crate::shim::econ::{BLOCK_GAS_LIMIT, TokenAmount};
use crate::shim::executor::Receipt;
use crate::shim::fvm_shared_latest::error::ExitCode;
use crate::shim::message::Message;
use anyhow::{Context as _, bail, ensure};
use jsonrpsee::types::ErrorObjectOwned;
use libtest_mimic::{Arguments, Trial};
use tokio::sync::OnceCell;

/// `SimpleCoin`, whose `getBalance(address)` reads one storage slot.
const SIMPLE_COIN_HEX: &str = include_str!("contracts/simple_coin/simple_coin.hex");
const GET_BALANCE_SIGNATURE: &str = "getBalance(address)";

/// `Filecoin.StateCall` gas limit parity tests
#[derive(Debug, clap::Args)]
pub struct StateCallTestCommand {}

impl StateCallTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let args = Arguments {
            test_threads: Some(1),
            ..Default::default()
        };
        libtest_mimic::run(&args, tests()).exit();
    }
}

fn tests() -> Vec<Trial> {
    vec![
        trial("state_call_runs_at_the_message_gas_limit", || {
            block_on(state_call_runs_at_the_message_gas_limit())
        }),
        trial("state_call_charges_no_fees", || {
            block_on(state_call_charges_no_fees())
        }),
    ]
}

/// `SimpleCoin`, deployed once per process.
async fn contract() -> anyhow::Result<&'static EthAddress> {
    static CONTRACT: OnceCell<EthAddress> = OnceCell::const_new();
    CONTRACT
        .get_or_try_init(|| deploy_contract("SimpleCoin", SIMPLE_COIN_HEX))
        .await
}

/// `getBalance(sender)` on `SimpleCoin` from the sender, with the given gas limit.
async fn get_balance_message(gas_limit: u64) -> anyhow::Result<Message> {
    let (from, to) = tokio::try_join!(sender(), contract())?;
    let msg = Message::try_from(EthCallMessage {
        from: Some(from.eth),
        to: Some(*to),
        data: Some(EthBytes(calldata(
            GET_BALANCE_SIGNATURE,
            &abi_address_word(from.eth),
        ))),
        ..Default::default()
    })?;
    Ok(msg.into_builder().gas_limit(gas_limit).build())
}

fn receipt_of(result: &ApiInvocResult) -> anyhow::Result<&Receipt> {
    result.msg_rct.as_ref().context("no receipt in the result")
}

/// A tipset both nodes have executed, so the two simulations start from the same state.
async fn common_tipset(forest: &Client, lotus: &Client) -> anyhow::Result<ApiTipsetKey> {
    let (forest_head, lotus_head) = tokio::try_join!(
        async { anyhow::Ok(forest.call(ChainHead::request(())?).await?) },
        async { anyhow::Ok(lotus.call(ChainHead::request(())?).await?) },
    )?;
    let epoch = forest_head.epoch().min(lotus_head.epoch());
    let tipset = forest
        .call(ChainGetTipSetByHeight::request((
            epoch,
            ApiTipsetKey(None),
        ))?)
        .await?;
    Ok(tipset.key().into())
}

/// Both nodes and a tipset both have executed, so every call starts from the same state.
struct Devnet {
    forest: Client,
    lotus: Client,
    tipset: ApiTipsetKey,
}

impl Devnet {
    /// Deploys the contract first, so the pinned tipset already has it on both nodes.
    async fn pinned() -> anyhow::Result<Self> {
        contract().await?;
        let (forest, lotus) = (forest_client()?, lotus_client()?);
        let tipset = common_tipset(&forest, &lotus).await?;
        Ok(Self {
            forest,
            lotus,
            tipset,
        })
    }

    fn request(&self, msg: Message) -> anyhow::Result<Request<ApiInvocResult>> {
        Ok(StateCall::request((msg, self.tipset.clone()))?)
    }

    /// Runs `msg` on both nodes, which must return the same result with `exit_code`.
    async fn expect(&self, msg: Message, exit_code: ExitCode) -> anyhow::Result<ApiInvocResult> {
        let request = self.request(msg)?;
        let (forest, lotus) = tokio::try_join!(
            async { anyhow::Ok(self.forest.call(request.clone()).await?) },
            async { anyhow::Ok(self.lotus.call(request.clone()).await?) },
        )?;
        for (node, result) in [("forest", &forest), ("lotus", &lotus)] {
            let exit = receipt_of(result).with_context(|| node)?.exit_code();
            ensure!(
                exit == exit_code,
                "{node}: expected exit code {exit_code}, got {exit} ({})",
                result.error
            );
        }
        ensure!(
            forest.msg == lotus.msg,
            "echoed messages differ: forest {:?}, lotus {:?}",
            forest.msg,
            lotus.msg
        );
        ensure!(
            forest.msg_cid == lotus.msg_cid,
            "message CIDs differ: forest {}, lotus {}",
            forest.msg_cid,
            lotus.msg_cid
        );
        ensure!(
            forest.msg_rct == lotus.msg_rct,
            "receipts differ: forest {:?}, lotus {:?}",
            forest.msg_rct,
            lotus.msg_rct
        );
        ensure!(
            forest.error == lotus.error,
            "errors differ: forest `{}`, lotus `{}`",
            forest.error,
            lotus.error
        );
        ensure!(
            forest.gas_cost == lotus.gas_cost,
            "gas costs differ: forest {:?}, lotus {:?}",
            forest.gas_cost,
            lotus.gas_cost
        );
        ensure!(
            forest.execution_trace == lotus.execution_trace,
            "execution traces differ"
        );
        Ok(forest)
    }

    /// Sends `msg` to both nodes, which must both answer with a JSON-RPC error.
    async fn expect_rejected(
        &self,
        msg: Message,
    ) -> anyhow::Result<(ErrorObjectOwned, ErrorObjectOwned)> {
        let request = self.request(msg)?;
        tokio::try_join!(
            rejection("forest", &self.forest, request.clone()),
            rejection("lotus", &self.lotus, request)
        )
    }
}

async fn rejection(
    node: &str,
    client: &Client,
    request: Request<ApiInvocResult>,
) -> anyhow::Result<ErrorObjectOwned> {
    let err = match client.call(request).await {
        Ok(result) => bail!("{node} ran a message it must reject: {result:?}"),
        Err(e) => anyhow::Error::from(e),
    };
    rpc_call_err(&err)
        .cloned()
        .with_context(|| format!("{node}: expected a JSON-RPC error, got {err:?}"))
}

/// Both nodes run the message at its own gas limit, treat `0` as the block gas limit, and reject a
/// limit above it before running anything.
async fn state_call_runs_at_the_message_gas_limit() -> anyhow::Result<()> {
    let devnet = Devnet::pinned().await?;
    let ok = ExitCode::OK;

    // Gas 0 means the cap, the block gas limit by default.
    let result = devnet.expect(get_balance_message(0).await?, ok).await?;
    ensure!(
        result.msg.gas_limit() == BLOCK_GAS_LIMIT,
        "expected gas 0 to run at the block gas limit, got {}",
        result.msg.gas_limit()
    );
    let gas_used = receipt_of(&result)?.gas_used();
    ensure!(
        gas_used > 1,
        "expected getBalance to use gas, got {gas_used}"
    );
    eprintln!("getBalance used {gas_used} gas");

    // Any other limit is the one the message runs at: enough gas succeeds, one unit short runs out.
    let limits = [
        ("exactly the gas used", gas_used, ok),
        ("one unit short", gas_used - 1, ExitCode::SYS_OUT_OF_GAS),
        ("the block gas limit", BLOCK_GAS_LIMIT, ok),
    ];
    for (label, limit, exit_code) in limits {
        let result = devnet
            .expect(get_balance_message(limit).await?, exit_code)
            .await
            .with_context(|| format!("at {label} ({limit})"))?;
        ensure!(
            result.msg.gas_limit() == limit,
            "{label}: expected the message to run at {limit}, got {}",
            result.msg.gas_limit()
        );
    }

    // Above the cap is rejected before anything runs, with Lotus's text.
    let expected = format!(
        "StateCall gas limit {} exceeds configured cap {BLOCK_GAS_LIMIT}",
        BLOCK_GAS_LIMIT + 1
    );
    let (forest, lotus) = devnet
        .expect_rejected(get_balance_message(BLOCK_GAS_LIMIT + 1).await?)
        .await?;
    for (node, err) in [("forest", &forest), ("lotus", &lotus)] {
        ensure!(
            err.message().contains(&expected),
            "{node}: expected `{expected}`, got `{}`",
            err.message()
        );
    }
    Ok(())
}

/// A realistic limit changes nothing about fees: the call is applied without checking or charging
/// them, so fees the sender could never pay still run and come back echoed as given.
async fn state_call_charges_no_fees() -> anyhow::Result<()> {
    let devnet = Devnet::pinned().await?;
    let unpayable = TokenAmount::from_whole(1_000_000u64);
    let priced = get_balance_message(0)
        .await?
        .into_builder()
        .gas_fee_cap(unpayable.clone())
        .gas_premium(unpayable.clone())
        .build();
    let result = devnet.expect(priced, ExitCode::OK).await?;
    ensure!(
        result.msg.gas_fee_cap() == &unpayable && result.msg.gas_premium() == &unpayable,
        "expected the fees echoed as given, got {:?}",
        result.msg
    );
    Ok(())
}

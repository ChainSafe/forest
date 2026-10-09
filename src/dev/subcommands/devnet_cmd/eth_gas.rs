// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! `eth_estimateGas` parity tests against the Lotus node on the docker devnet, plus Forest-only
//! tests of a caller-supplied `gas` cap.
//!
//! [EIP-150] caps a `CALL` at 63/64 of remaining gas, so a nested call chain needs a far higher
//! gas *limit* than the gas it *uses*. Estimating from gas used alone therefore under-shoots,
//! and the estimate has to be probed and raised until it succeeds.
//!
//! [EIP-150]: https://github.com/ethereum/EIPs/blob/15f61ed0fda82ec86d8d6a872f6b874816f03d96/EIPS/eip-150.md#L32-L33

use crate::dev::subcommands::tests_cmd::helpers::*;
use crate::rpc::Client;
use crate::rpc::eth::errors::{
    EXECUTION_REVERTED_CODE, INVALID_INPUT_CODE, TRANSACTION_REJECTED_CODE,
};
use crate::rpc::eth::{
    BlockNumberOrHash, EthBigInt, EthUint64, Predefined,
    types::{EthAddress, EthBytes, EthCallMessage},
};
use crate::rpc::prelude::*;
use crate::shim::address::Address;
use crate::shim::econ::{BLOCK_GAS_LIMIT, TokenAmount};
use crate::utils::encoding::keccak_256;
use anyhow::{Context as _, ensure};
use jsonrpsee::types::error::INVALID_PARAMS_CODE;
use libtest_mimic::{Arguments, Failed, Trial};
use std::str::FromStr as _;
use tokio::sync::OnceCell;

/// `NestedGas`, whose `recurse(uint256)` calls itself that many times.
/// Regenerate with `contracts/compile.sh` after editing the source.
const NESTED_GAS_HEX: &str = include_str!("contracts/nested_gas/nested_gas.hex");
const RECURSE_SIGNATURE: &str = "recurse(uint256)";
/// Reverts explicitly unless given a large gas limit, so the estimator has to search past the revert.
const REQUIRES_HIGH_GAS_SIGNATURE: &str = "requiresHighGasLimit()";
/// The `gasleft()` bound in [`REQUIRES_HIGH_GAS_SIGNATURE`].
const REQUIRES_HIGH_GAS_THRESHOLD: u64 = 50_000_000;
const ALWAYS_REVERTS_SIGNATURE: &str = "alwaysReverts()";
const ALWAYS_REVERTS_REASON: &str = "always reverts";

/// Shallow enough that the 63/64 penalty stays inside any estimator's safety margin, so both
/// nodes must agree. Guards against a failure that is really "the two disagree about gas".
const CONTROL_DEPTH: u64 = 0;
/// Deep enough that the penalty is ~1.9x, well clear of the crossover measured around 40-60.
const NESTED_DEPTH: u64 = 100;
/// The nested call needs a gas limit in the hundreds of millions, and the devnet Lotus estimates
/// with fees, so a sender that cannot afford it makes its estimate saturate instead of converging.
const SENDER_FUND_AMT: &str = "10 FIL";
/// Enough for the sender to exist, far too little to pay for the nested call's gas.
const POOR_SENDER_FUND_AMT: &str = "1 nanoFIL";

/// `eth_estimateGas` parity and gas cap tests
#[derive(Debug, clap::Args)]
pub struct EthGasTestCommand {}

impl EthGasTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let args = Arguments {
            test_threads: Some(1),
            ..Default::default()
        };
        libtest_mimic::run(&args, tests()).exit();
    }
}

fn tests() -> Vec<Trial> {
    fn trial(name: &'static str, body: fn() -> anyhow::Result<()>) -> Trial {
        Trial::test(name, move || {
            body().map_err(|e| Failed::from(format!("{e:?}")))
        })
    }

    vec![
        trial("eth_estimate_gas_agrees_without_nesting", || {
            block_on(estimate_agrees(CONTROL_DEPTH))
        }),
        trial("eth_estimate_gas_agrees_with_nesting", || {
            block_on(estimate_agrees(NESTED_DEPTH))
        }),
        trial("eth_estimate_gas_is_sufficient_on_chain", || {
            block_on(estimate_is_sufficient_on_chain())
        }),
        trial("eth_estimate_gas_reports_a_non_gas_failure", || {
            block_on(estimate_reports_a_non_gas_failure())
        }),
        trial("eth_estimate_gas_honors_gas_cap", || {
            block_on(estimate_honors_gas_cap())
        }),
        trial(
            "eth_estimate_gas_under_cap_searches_past_gas_dependent_revert",
            || block_on(estimate_under_cap_searches_past_gas_dependent_revert()),
        ),
        trial(
            "eth_estimate_gas_without_cap_searches_past_gas_dependent_revert",
            || block_on(estimate_without_cap_searches_past_gas_dependent_revert()),
        ),
        trial(
            "eth_estimate_gas_ignores_sender_funds_without_price",
            || block_on(estimate_ignores_sender_funds_without_price()),
        ),
        trial("eth_estimate_gas_honors_gas_price", || {
            block_on(estimate_honors_gas_price())
        }),
    ]
}

/// The 4-byte Ethereum function selector: first 4 bytes of `keccak256(signature)`.
fn selector(signature: &str) -> Vec<u8> {
    keccak_256(signature.as_bytes())
        .get(..4)
        .expect("keccak256 is 32 bytes")
        .to_vec()
}

/// ABI calldata for `recurse(uint256)`: the selector followed by `depth` as a 32-byte word.
fn recurse_calldata(depth: u64) -> Vec<u8> {
    let mut out = selector(RECURSE_SIGNATURE);
    out.extend_from_slice(&ethereum_types::U256::from(depth).to_big_endian());
    out
}

/// Deploys `NestedGas` once per process.
async fn contract() -> anyhow::Result<&'static EthAddress> {
    static CONTRACT: OnceCell<EthAddress> = OnceCell::const_new();
    CONTRACT
        .get_or_try_init(|| async {
            let from = sender().await?;
            let deploy = forest_evm_deploy_hex(from, NESTED_GAS_HEX)?;
            let f4 = parse_f4_from_evm_deploy(&deploy)?;
            eprintln!("deployed NestedGas at {f4}");
            poll_until_actor_on("forest", f4, forest_client).await?;
            poll_until_actor_on("lotus", f4, lotus_client).await?;
            poll_until_next_epoch().await?;
            EthAddress::from_filecoin_address(&f4)
        })
        .await
}

/// Funded delegated sender. Lotus rejects estimates from an unfunded or non-`f4` address.
async fn sender() -> anyhow::Result<&'static str> {
    static SENDER: OnceCell<String> = OnceCell::const_new();
    Ok(SENDER
        .get_or_try_init(|| async {
            let (addr, _) = new_funded_delegated(SENDER_FUND_AMT).await?;
            import_lotus_wallet_into_forest(&addr)?;
            anyhow::Ok(addr)
        })
        .await?
        .as_str())
}

/// A new Lotus delegated wallet funded with `amount`, once both nodes see it.
async fn new_funded_delegated(amount: &str) -> anyhow::Result<(String, Address)> {
    let addr = lotus_exec(&["wallet", "new", "delegated"])?;
    let msg = send_from(
        &FOREST_TEST_PRELOADED_ADDRESS,
        &addr,
        amount,
        Backend::Local,
    )?;
    eprintln!("funding {addr} with {amount}, msg: {msg}");
    let balance = poll_until_funded(&addr, Backend::Local).await?;
    eprintln!("{addr} funded, balance: {balance}");
    let parsed = Address::from_str(&addr).context("parsing the funded address")?;
    poll_until_actor_on("lotus", parsed, lotus_client).await?;
    Ok((addr, parsed))
}

/// A call from the funded sender to the deployed contract.
async fn call_message(calldata: Vec<u8>, gas: Option<u64>) -> anyhow::Result<EthCallMessage> {
    let (from, to) = tokio::try_join!(sender(), contract())?;
    let from = Address::from_str(from).context("parsing the sender address")?;
    Ok(EthCallMessage {
        from: Some(EthAddress::from_filecoin_address(&from)?),
        to: Some(*to),
        data: Some(EthBytes(calldata)),
        gas: gas.map(EthUint64),
        ..Default::default()
    })
}

async fn estimate_msg(
    client: &Client,
    msg: EthCallMessage,
    block: BlockNumberOrHash,
) -> anyhow::Result<u64> {
    let gas = client
        .call(EthEstimateGas::request((msg, Some(block)))?)
        .await?;
    Ok(gas.0)
}

async fn estimate(
    client: &Client,
    calldata: Vec<u8>,
    block: BlockNumberOrHash,
    gas: Option<u64>,
) -> anyhow::Result<u64> {
    estimate_msg(client, call_message(calldata, gas).await?, block).await
}

/// Estimating `msg` must fail with this JSON-RPC code and exactly the `expected` message.
async fn expect_estimate_error(
    client: &Client,
    msg: EthCallMessage,
    block: BlockNumberOrHash,
    code: i32,
    expected: &str,
) -> anyhow::Result<()> {
    let err = match estimate_msg(client, msg.clone(), block).await {
        Ok(gas) => anyhow::bail!("returned {gas} for a call that must fail: {msg:?}"),
        Err(e) => e,
    };
    let obj = rpc_call_err(&err)
        .with_context(|| format!("expected a JSON-RPC error for {msg:?}: {err:?}"))?;
    ensure!(
        obj.code() == code && obj.message() == expected,
        "expected code {code} `{expected}` for {msg:?}, got code {} `{}`",
        obj.code(),
        obj.message()
    );
    Ok(())
}

/// `eth_call` must succeed with `msg`, proving an estimate put in its `gas` is enough.
async fn expect_call_ok(
    client: &Client,
    msg: EthCallMessage,
    block: BlockNumberOrHash,
) -> anyhow::Result<()> {
    client
        .call(EthCall::request((msg.clone(), block))?)
        .await
        .with_context(|| format!("eth_call failed for {msg:?}"))?;
    Ok(())
}

/// A height both nodes have already executed. `Latest` is resolved per node, so at an epoch
/// boundary or under slight sync skew the two could pick different tipsets; pinning both to the
/// lower of their heads makes the cross-node comparison deterministic.
async fn common_block_number(a: &Client, b: &Client) -> anyhow::Result<i64> {
    let (head_a, head_b) = tokio::try_join!(
        async { anyhow::Ok(a.call(EthBlockNumber::request(())?).await?) },
        async { anyhow::Ok(b.call(EthBlockNumber::request(())?).await?) },
    )?;
    Ok(head_a.0.min(head_b.0) as i64)
}

async fn poll_until_next_epoch() -> anyhow::Result<()> {
    let current_epoch = common_block_number(&forest_client()?, &lotus_client()?).await?;
    poll("both nodes one epoch past deploy", || async {
        Ok(
            (common_block_number(&forest_client()?, &lotus_client()?).await? > current_epoch)
                .then_some(()),
        )
    })
    .await
}

/// Deploy + fund, build both node clients, and pin a block height both have executed. Sampling the
/// height only after the deploy/fund guarantees the pinned tipset already contains the contract and
/// sender on both nodes (the funding poll also lets both catch up to the deploy).
async fn pinned_common_block() -> anyhow::Result<(Client, Client, i64)> {
    contract().await?;
    let (forest_c, lotus_c) = (forest_client()?, lotus_client()?);
    let block = common_block_number(&forest_c, &lotus_c).await?;
    Ok((forest_c, lotus_c, block))
}

/// Forest and Lotus must return the same estimate.
async fn estimate_agrees(depth: u64) -> anyhow::Result<()> {
    let (forest_c, lotus_c, block) = pinned_common_block().await?;
    let (forest, lotus) = tokio::try_join!(
        async {
            estimate(
                &forest_c,
                recurse_calldata(depth),
                BlockNumberOrHash::from_block_number(block),
                None,
            )
            .await
            .context("EthEstimateGas on forest")
        },
        async {
            estimate(
                &lotus_c,
                recurse_calldata(depth),
                BlockNumberOrHash::from_block_number(block),
                None,
            )
            .await
            .context("EthEstimateGas on lotus")
        },
    )?;
    eprintln!("depth={depth} block={block} forest={forest} lotus={lotus}");
    ensure!(
        forest == lotus,
        "eth_estimateGas disagrees at recursion depth {depth} (block {block}): forest={forest} lotus={lotus}"
    );
    Ok(())
}

/// The estimate Forest returns must actually be enough to land the transaction.
async fn estimate_is_sufficient_on_chain() -> anyhow::Result<()> {
    let forest = forest_client()?;
    // No cross-node comparison here, so `Latest` is fine: the estimate must reflect the same
    // fresh state the following `forest-wallet send` executes against.
    let estimate = estimate(
        &forest,
        recurse_calldata(NESTED_DEPTH),
        BlockNumberOrHash::PredefinedBlock(Predefined::Latest),
        None,
    )
    .await?;
    let from = sender().await?;
    // `forest-wallet send` infers `InvokeContract` and CBOR-wraps the params when the sender is an
    // eth account, and rejects an explicit `--method`, so pass the bare calldata.
    let cid = wallet_send_calldata(
        from,
        contract().await?,
        &recurse_calldata(NESTED_DEPTH),
        estimate,
    )
    .await
    .with_context(|| {
        format!(
            "a transaction submitted at forest's own eth_estimateGas value ({estimate}) failed \
             on chain; the estimate is not a usable gas limit"
        )
    })?;
    eprintln!("submitted at forest's estimate {estimate}: {cid}");
    Ok(())
}

/// A call that reverts at any gas limit must be reported, not answered with a gas value.
async fn estimate_reports_a_non_gas_failure() -> anyhow::Result<()> {
    let (forest_c, lotus_c, block) = pinned_common_block().await?;
    for (node, client) in [("forest", &forest_c), ("lotus", &lotus_c)] {
        let err = match estimate(
            client,
            selector(ALWAYS_REVERTS_SIGNATURE),
            BlockNumberOrHash::from_block_number(block),
            None,
        )
        .await
        {
            Ok(gas) => anyhow::bail!(
                "{node} returned an estimate ({gas}) for a message that reverts at any limit"
            ),
            Err(e) => e,
        };
        let obj = rpc_call_err(&err).with_context(|| {
            format!("{node} returned a non-JSON-RPC error, cannot check parity: {err:?}")
        })?;
        eprintln!(
            "{node} rejected the call: code={} has_data={} msg={}",
            obj.code(),
            obj.data().is_some(),
            obj.message()
        );
        ensure!(
            obj.message().contains(ALWAYS_REVERTS_REASON),
            "{node} rejected the call without naming the revert reason `{ALWAYS_REVERTS_REASON}`: {}",
            obj.message()
        );

        ensure!(
            obj.code() == EXECUTION_REVERTED_CODE,
            "{node} rejected with code {}, expected execution-reverted {EXECUTION_REVERTED_CODE}: {}",
            obj.code(),
            obj.message()
        );
        ensure!(
            obj.data().is_some(),
            "{node} rejected without revert data; eth clients cannot ABI-decode the reason: {}",
            obj.message()
        );
    }
    Ok(())
}

/// A caller-supplied `gas` bounds the estimate.
async fn estimate_honors_gas_cap() -> anyhow::Result<()> {
    let (forest, _, block) = pinned_common_block().await?;
    let block = BlockNumberOrHash::from_block_number(block);
    let calldata = recurse_calldata(NESTED_DEPTH);
    let uncapped = estimate(&forest, calldata.clone(), block.clone(), None).await?;

    let estimate_with_spare_cap =
        estimate(&forest, calldata.clone(), block.clone(), Some(uncapped * 2)).await?;
    ensure!(
        estimate_with_spare_cap == uncapped,
        "a cap above the need changed the estimate: capped={estimate_with_spare_cap} uncapped={uncapped}"
    );

    // Just under the estimate still covers the need, so the result must stay within the cap and work.
    let tight = uncapped - 1;
    let estimate_with_tight_cap =
        estimate(&forest, calldata.clone(), block.clone(), Some(tight)).await?;
    ensure!(
        estimate_with_tight_cap <= tight,
        "the estimate {estimate_with_tight_cap} exceeds the cap {tight}"
    );
    let call = call_message(calldata.clone(), Some(estimate_with_tight_cap)).await?;
    expect_call_ok(&forest, call, block.clone()).await?;

    // Half the estimate is short of what the nesting needs.
    let half = uncapped / 2;
    expect_estimate_error(
        &forest,
        call_message(calldata.clone(), Some(half)).await?,
        block.clone(),
        TRANSACTION_REJECTED_CODE,
        &format!("out of gas: gas required exceeds: {half}"),
    )
    .await?;
    // Below the inclusion cost, preflight rejects the message before it runs.
    expect_estimate_error(
        &forest,
        call_message(calldata, Some(21_000)).await?,
        block,
        INVALID_INPUT_CODE,
        "gas required exceeds allowance (21000)",
    )
    .await
}

/// Under a cap, a revert that more gas fixes is searched past instead of reported. Forest only: the devnet Lotus reports it.
async fn estimate_under_cap_searches_past_gas_dependent_revert() -> anyhow::Result<()> {
    let (forest, _, block) = pinned_common_block().await?;
    let block = BlockNumberOrHash::from_block_number(block);
    let calldata = selector(REQUIRES_HIGH_GAS_SIGNATURE);

    let gas = estimate(
        &forest,
        calldata.clone(),
        block.clone(),
        Some(BLOCK_GAS_LIMIT),
    )
    .await?;
    ensure!(
        gas > REQUIRES_HIGH_GAS_THRESHOLD,
        "expected an estimate above {REQUIRES_HIGH_GAS_THRESHOLD}, got {gas}"
    );
    let call = call_message(calldata.clone(), Some(gas)).await?;
    expect_call_ok(&forest, call, block.clone()).await?;

    let cap = REQUIRES_HIGH_GAS_THRESHOLD / 2;
    expect_estimate_error(
        &forest,
        call_message(calldata, Some(cap)).await?,
        block,
        TRANSACTION_REJECTED_CODE,
        &format!("out of gas: gas required exceeds: {cap}"),
    )
    .await
}

/// Without a cap too, a revert that more gas fixes is searched past. Forest only: the devnet Lotus reports it.
async fn estimate_without_cap_searches_past_gas_dependent_revert() -> anyhow::Result<()> {
    let (forest, _, block) = pinned_common_block().await?;
    let block = BlockNumberOrHash::from_block_number(block);
    let calldata = selector(REQUIRES_HIGH_GAS_SIGNATURE);

    let gas = estimate(&forest, calldata.clone(), block.clone(), None).await?;
    ensure!(
        gas > REQUIRES_HIGH_GAS_THRESHOLD && gas <= BLOCK_GAS_LIMIT,
        "expected an estimate in ({REQUIRES_HIGH_GAS_THRESHOLD}, {BLOCK_GAS_LIMIT}], got {gas}"
    );
    expect_call_ok(&forest, call_message(calldata, Some(gas)).await?, block).await
}

/// Funded with [`POOR_SENDER_FUND_AMT`], once per process.
async fn poor_sender() -> anyhow::Result<&'static EthAddress> {
    static POOR_SENDER: OnceCell<EthAddress> = OnceCell::const_new();
    POOR_SENDER
        .get_or_try_init(|| async {
            let (_, addr) = new_funded_delegated(POOR_SENDER_FUND_AMT).await?;
            EthAddress::from_filecoin_address(&addr)
        })
        .await
}

/// Estimates run with zero fees, so a sender that cannot pay for the gas still gets one, with or
/// without a cap. Forest only: the devnet Lotus estimates with fees.
async fn estimate_ignores_sender_funds_without_price() -> anyhow::Result<()> {
    let poor = *poor_sender().await?;
    let forest = forest_client()?;
    let block = BlockNumberOrHash::PredefinedBlock(Predefined::Latest);
    let calldata = recurse_calldata(NESTED_DEPTH);
    for gas in [None, Some(BLOCK_GAS_LIMIT)] {
        let msg = EthCallMessage {
            from: Some(poor),
            ..call_message(calldata.clone(), gas).await?
        };
        let estimate = estimate_msg(&forest, msg.clone(), block.clone())
            .await
            .with_context(|| {
                format!("estimating for a sender that cannot pay fees, gas={gas:?}")
            })?;
        ensure!(
            estimate < BLOCK_GAS_LIMIT,
            "the estimate {estimate} saturated instead of converging, gas={gas:?}"
        );
        let msg = EthCallMessage {
            gas: Some(EthUint64(estimate)),
            ..msg
        };
        expect_call_ok(&forest, msg, block.clone()).await?;
    }
    Ok(())
}

/// A `gasPrice` or `maxFeePerGas` limits the estimate to the gas the sender can pay for. Forest
/// only: the devnet Lotus ignores both.
async fn estimate_honors_gas_price() -> anyhow::Result<()> {
    let (forest, _, block) = pinned_common_block().await?;
    let block = BlockNumberOrHash::from_block_number(block);
    let calldata = recurse_calldata(NESTED_DEPTH);
    let base = call_message(calldata, None).await?;
    let uncapped = estimate_msg(&forest, base.clone(), block.clone()).await?;

    // One attoFIL per gas leaves the funded sender far more than the block gas limit.
    let affordable = estimate_msg(
        &forest,
        EthCallMessage {
            gas_price: Some(EthBigInt::from(1)),
            ..base.clone()
        },
        block.clone(),
    )
    .await?;
    ensure!(
        affordable == uncapped,
        "an affordable price changed the estimate: priced={affordable} unpriced={uncapped}"
    );

    let from = base.from.context("the call has no sender")?;
    let balance = TokenAmount::from(
        forest
            .call(EthGetBalance::request((from, block.clone()))?)
            .await?,
    );

    // At 10^13 attoFIL per gas the funded sender affords about a million gas, far below the need.
    let max_fee = TokenAmount::from_atto(10_u64.pow(13));
    let allowance = balance.div_floor(max_fee.atto().clone());
    expect_estimate_error(
        &forest,
        EthCallMessage {
            max_fee_per_gas: Some(max_fee.into()),
            ..base.clone()
        },
        block.clone(),
        TRANSACTION_REJECTED_CODE,
        &format!("out of gas: gas required exceeds: {}", allowance.atto()),
    )
    .await?;
    expect_estimate_error(
        &forest,
        EthCallMessage {
            gas_price: Some(EthBigInt::from(1)),
            max_fee_per_gas: Some(EthBigInt::from(1)),
            ..base.clone()
        },
        block.clone(),
        INVALID_PARAMS_CODE,
        "both gasPrice and (maxFeePerGas or maxPriorityFeePerGas) specified",
    )
    .await?;
    let value = TokenAmount::from_whole(1_000_000);
    expect_estimate_error(
        &forest,
        EthCallMessage {
            gas_price: Some(EthBigInt::from(1)),
            value: Some((&value).into()),
            ..base
        },
        block,
        TRANSACTION_REJECTED_CODE,
        &format!(
            "insufficient funds for gas * price + value: have {} want {}",
            balance.atto(),
            value.atto()
        ),
    )
    .await
}

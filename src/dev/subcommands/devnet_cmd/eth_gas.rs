// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! `eth_estimateGas` parity tests against the Lotus node on the docker devnet.
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
    BlockNumberOrHash, EthUint64, Predefined,
    types::{EthAddress, EthBytes, EthCallMessage},
};
use crate::rpc::prelude::*;
use crate::shim::address::Address;
use crate::shim::econ::BLOCK_GAS_LIMIT;
use crate::utils::encoding::keccak_256;
use anyhow::{Context as _, ensure};
use jsonrpsee::core::ClientError;
use libtest_mimic::{Arguments, Failed, Trial};
use std::str::FromStr as _;
use tokio::sync::OnceCell;

/// `NestedGas`, whose `recurse(uint256)` calls itself that many times.
/// Regenerate with `contracts/compile.sh` after editing the source.
const NESTED_GAS_HEX: &str = include_str!("contracts/nested_gas/nested_gas.hex");
const RECURSE_SIGNATURE: &str = "recurse(uint256)";
/// Reverts explicitly unless given a large gas limit, so estimating it fails for a reason no
/// amount of extra gas can be shown to fix.
const REQUIRES_HIGH_GAS_SIGNATURE: &str = "requiresHighGasLimit()";
/// The `gasleft()` bound in [`REQUIRES_HIGH_GAS_SIGNATURE`].
const REQUIRES_HIGH_GAS_THRESHOLD: u64 = 50_000_000;
/// The `require` string in [`REQUIRES_HIGH_GAS_SIGNATURE`].
const REVERT_REASON: &str = "gas limit too low";

/// Shallow enough that the 63/64 penalty stays inside any estimator's safety margin, so both
/// nodes must agree. Guards against a failure that is really "the two disagree about gas".
const CONTROL_DEPTH: u64 = 0;
/// Deep enough that the penalty is ~1.9x, well clear of the crossover measured around 40-60.
const NESTED_DEPTH: u64 = 100;
/// The nested call needs a gas limit in the hundreds of millions, and a sender that cannot
/// afford it makes the estimate saturate at the block gas limit instead of converging.
const SENDER_FUND_AMT: &str = "10 FIL";

/// `eth_estimateGas` parity tests
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
            let addr = lotus_exec(&["wallet", "new", "delegated"])?;
            let msg = send_from(
                &FOREST_TEST_PRELOADED_ADDRESS,
                &addr,
                SENDER_FUND_AMT,
                Backend::Local,
            )?;
            eprintln!("funding sender {addr} with {SENDER_FUND_AMT}, msg: {msg}");
            let balance = poll_until_funded(&addr, Backend::Local).await?;
            eprintln!("sender {addr} funded balance: {balance}");
            let parsed = Address::from_str(&addr).context("parsing the sender address")?;
            poll_until_actor_on("lotus", parsed, lotus_client).await?;
            import_lotus_wallet_into_forest(&addr)?;
            anyhow::Ok(addr)
        })
        .await?
        .as_str())
}

async fn estimate(
    client: &Client,
    calldata: Vec<u8>,
    block: BlockNumberOrHash,
    gas: Option<u64>,
) -> anyhow::Result<u64> {
    let (from, to) = tokio::try_join!(sender(), contract())?;
    let from = Address::from_str(from).context("parsing the sender address")?;
    let msg = EthCallMessage {
        from: Some(EthAddress::from_filecoin_address(&from)?),
        to: Some(*to),
        data: Some(EthBytes(calldata)),
        gas: gas.map(EthUint64),
        ..Default::default()
    };
    let gas = client
        .call(EthEstimateGas::request((msg, Some(block)))?)
        .await?;
    Ok(gas.0)
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

/// A failure that raising the gas limit cannot be shown to fix must be reported, not searched
/// around. This is the companion of [`estimate_agrees`]: it pins the branch that decides whether
/// a failed probe means "needs more gas" or "is simply broken".
async fn estimate_reports_a_non_gas_failure() -> anyhow::Result<()> {
    let (forest_c, lotus_c, block) = pinned_common_block().await?;
    for (node, client) in [("forest", &forest_c), ("lotus", &lotus_c)] {
        let err = match estimate(
            client,
            selector(REQUIRES_HIGH_GAS_SIGNATURE),
            BlockNumberOrHash::from_block_number(block),
            None,
        )
        .await
        {
            Ok(gas) => anyhow::bail!(
                "{node} returned an estimate ({gas}) for a message that reverts at that limit; \
                 a non-gas failure must be reported, not answered with a gas value"
            ),
            Err(e) => e,
        };
        let Some(ClientError::Call(obj)) = err.downcast_ref::<ClientError>() else {
            anyhow::bail!("{node} returned a non-JSON-RPC error, cannot check parity: {err:?}");
        };
        eprintln!(
            "{node} rejected the call: code={} has_data={} msg={}",
            obj.code(),
            obj.data().is_some(),
            obj.message()
        );
        ensure!(
            obj.message().contains(REVERT_REASON),
            "{node} rejected the call without naming the revert reason `{REVERT_REASON}`: {}",
            obj.message()
        );

        // Forest returns eth-standard `execution reverted` (code 3) + data, matching current Lotus.
        // The devnet's Lotus image predates that refactor (generic code, no data), so code/data
        // parity is pinned on Forest alone.
        if node == "forest" {
            ensure!(
                obj.code() == EXECUTION_REVERTED_CODE,
                "forest rejected with code {}, expected execution-reverted {EXECUTION_REVERTED_CODE}: {}",
                obj.code(),
                obj.message()
            );
            ensure!(
                obj.data().is_some(),
                "forest rejected without revert data; eth clients cannot ABI-decode the reason: {}",
                obj.message()
            );
        }
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

    // Half the estimate is short of what the nesting needs; `21000` is below the inclusion cost, so preflight rejects it.
    for (cap, code, expected) in [
        (
            uncapped / 2,
            TRANSACTION_REJECTED_CODE,
            format!("out of gas: gas required exceeds: {}", uncapped / 2),
        ),
        (
            21_000,
            INVALID_INPUT_CODE,
            "gas required exceeds allowance (21000)".to_string(),
        ),
    ] {
        let err = match estimate(&forest, calldata.clone(), block.clone(), Some(cap)).await {
            Ok(gas) => {
                anyhow::bail!("returned {gas} for a call that does not fit in a cap of {cap}")
            }
            Err(e) => e,
        };
        let Some(ClientError::Call(obj)) = err.downcast_ref::<ClientError>() else {
            anyhow::bail!("expected a JSON-RPC error for a cap of {cap}: {err:?}");
        };
        ensure!(
            obj.code() == code && obj.message() == expected,
            "expected code {code} `{expected}` for a cap of {cap}, got code {} `{}`",
            obj.code(),
            obj.message()
        );
    }
    Ok(())
}

/// Under a cap, a revert that more gas fixes is searched past instead of reported. Forest only: Lotus reports it.
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
        gas > REQUIRES_HIGH_GAS_THRESHOLD && gas <= BLOCK_GAS_LIMIT,
        "expected an estimate in ({REQUIRES_HIGH_GAS_THRESHOLD}, {BLOCK_GAS_LIMIT}], got {gas}"
    );
    let (from, to) = tokio::try_join!(sender(), contract())?;
    let from = Address::from_str(from).context("parsing the sender address")?;
    let call = EthCallMessage {
        from: Some(EthAddress::from_filecoin_address(&from)?),
        to: Some(*to),
        data: Some(EthBytes(calldata.clone())),
        gas: Some(EthUint64(gas)),
        ..Default::default()
    };
    forest
        .call(EthCall::request((call, block.clone()))?)
        .await
        .with_context(|| format!("eth_call at the estimate {gas} failed"))?;

    let cap = REQUIRES_HIGH_GAS_THRESHOLD / 2;
    let err = match estimate(&forest, calldata, block, Some(cap)).await {
        Ok(gas) => anyhow::bail!("returned {gas} for a call that reverts under a cap of {cap}"),
        Err(e) => e,
    };
    let Some(ClientError::Call(obj)) = err.downcast_ref::<ClientError>() else {
        anyhow::bail!("expected a JSON-RPC error for a cap of {cap}: {err:?}");
    };
    let expected = format!("out of gas: gas required exceeds: {cap}");
    ensure!(
        obj.code() == TRANSACTION_REJECTED_CODE && obj.message() == expected,
        "expected code {TRANSACTION_REJECTED_CODE} `{expected}`, got code {} `{}`",
        obj.code(),
        obj.message()
    );
    Ok(())
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! `eth_estimateGas` parity tests on the docker devnet. Every case runs against Forest and the
//! Lotus node, which must agree on the estimate, or on the error code and message.
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
use crate::rpc::types::ApiTipsetKey;
use crate::shim::econ::{BLOCK_GAS_LIMIT, TokenAmount};
use crate::utils::encoding::keccak_256;
use anyhow::{Context as _, bail, ensure};
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::types::error::INVALID_PARAMS_CODE;
use libtest_mimic::{Arguments, Failed, Trial};
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
const BOTH_PRICES_MESSAGE: &str =
    "both gasPrice and (maxFeePerGas or maxPriorityFeePerGas) specified";

/// Shallow enough that the 63/64 penalty stays inside any estimator's safety margin, so both
/// nodes must agree. Guards against a failure that is really "the two disagree about gas".
const CONTROL_DEPTH: u64 = 0;
/// Deep enough that the penalty is ~1.9x, well clear of the crossover measured around 40-60.
const NESTED_DEPTH: u64 = 100;
/// Pays for the on-chain submit and lets the priced estimates afford the nested call.
const SENDER_FUND_AMT: &str = "10 FIL";
/// Enough for the sender to exist, far too little to pay for the nested call's gas.
const POOR_SENDER_FUND_AMT: &str = "1 nanoFIL";

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
            "eth_estimate_gas_searches_past_gas_dependent_revert",
            || block_on(estimate_searches_past_gas_dependent_revert()),
        ),
        trial(
            "eth_estimate_gas_ignores_sender_funds_without_price",
            || block_on(estimate_ignores_sender_funds_without_price()),
        ),
        trial("eth_estimate_gas_price_limits_gas", || {
            block_on(estimate_price_limits_gas())
        }),
        trial("eth_estimate_gas_rejects_invalid_prices", || {
            block_on(estimate_rejects_invalid_prices())
        }),
        trial("eth_estimate_gas_reads_balance_after_the_block", || {
            block_on(estimate_reads_balance_after_the_block())
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
            let deploy = forest_evm_deploy_hex(&sender().await?.cli, NESTED_GAS_HEX)?;
            let f4 = parse_f4_from_evm_deploy(&deploy)?;
            eprintln!("deployed NestedGas at {f4}");
            poll_until_actor_on("forest", f4, forest_client).await?;
            poll_until_actor_on("lotus", f4, lotus_client).await?;
            poll_until_next_epoch().await?;
            EthAddress::from_filecoin_address(&f4)
        })
        .await
}

/// The funded sender, created once per process and known to both nodes and both wallets.
async fn sender() -> anyhow::Result<&'static Wallet> {
    static SENDER: OnceCell<Wallet> = OnceCell::const_new();
    SENDER
        .get_or_try_init(|| async {
            let wallet = new_funded(SENDER_FUND_AMT).await?;
            import_lotus_wallet_into_forest(&wallet.cli)?;
            Ok(wallet)
        })
        .await
}

/// A call from the funded sender to the deployed contract.
async fn call_message(calldata: Vec<u8>, gas: Option<u64>) -> anyhow::Result<EthCallMessage> {
    let (from, to) = tokio::try_join!(sender(), contract())?;
    Ok(EthCallMessage {
        from: Some(from.eth),
        to: Some(*to),
        data: Some(EthBytes(calldata)),
        gas: gas.map(EthUint64),
        ..Default::default()
    })
}

async fn estimate(
    client: &Client,
    msg: EthCallMessage,
    block: BlockNumberOrHash,
) -> anyhow::Result<u64> {
    Ok(client
        .call(EthEstimateGas::request((msg, Some(block)))?)
        .await?
        .0)
}

/// The JSON-RPC error `client` answers an estimate that must fail with.
async fn estimate_error(
    client: &Client,
    msg: EthCallMessage,
    block: BlockNumberOrHash,
) -> anyhow::Result<ErrorObjectOwned> {
    let err = match estimate(client, msg.clone(), block).await {
        Ok(gas) => bail!("returned {gas} for a call that must fail: {msg:?}"),
        Err(e) => e,
    };
    rpc_call_err(&err)
        .cloned()
        .with_context(|| format!("expected a JSON-RPC error for {msg:?}: {err:?}"))
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

/// Both nodes and a block both have executed, so every comparison runs on the same state.
struct Devnet {
    forest: Client,
    lotus: Client,
    block: BlockNumberOrHash,
}

impl Devnet {
    /// Deploys the contract and funds the sender first, so the pinned block already has both.
    async fn pinned() -> anyhow::Result<Self> {
        contract().await?;
        let (forest, lotus) = (forest_client()?, lotus_client()?);
        let block = common_block_number(&forest, &lotus).await?;
        Ok(Self {
            forest,
            lotus,
            block: BlockNumberOrHash::from_block_number(block),
        })
    }

    fn nodes(&self) -> [(&'static str, &Client); 2] {
        [("forest", &self.forest), ("lotus", &self.lotus)]
    }

    /// Estimates `msg` on both nodes, which must agree.
    async fn agreed_estimate(&self, msg: EthCallMessage) -> anyhow::Result<u64> {
        let (forest, lotus) = tokio::try_join!(
            estimate(&self.forest, msg.clone(), self.block.clone()),
            estimate(&self.lotus, msg.clone(), self.block.clone()),
        )?;
        ensure!(
            forest == lotus,
            "eth_estimateGas disagrees: forest={forest} lotus={lotus} for {msg:?}"
        );
        Ok(forest)
    }

    /// Estimating `msg` must fail on both nodes with `code` and exactly `message`.
    async fn expect_estimate_error(
        &self,
        msg: EthCallMessage,
        code: i32,
        message: &str,
    ) -> anyhow::Result<()> {
        for (node, client) in self.nodes() {
            let err = estimate_error(client, msg.clone(), self.block.clone()).await?;
            ensure!(
                err.code() == code && err.message() == message,
                "{node}: expected code {code} `{message}` for {msg:?}, got code {} `{}`",
                err.code(),
                err.message()
            );
        }
        Ok(())
    }

    /// `eth_call` with `msg` must succeed on both nodes, proving an estimate put in its `gas` is enough.
    async fn expect_call_ok(&self, msg: EthCallMessage) -> anyhow::Result<()> {
        for (node, client) in self.nodes() {
            client
                .call(EthCall::request((msg.clone(), self.block.clone()))?)
                .await
                .with_context(|| format!("{node}: eth_call failed for {msg:?}"))?;
        }
        Ok(())
    }
}

/// Forest and Lotus must return the same estimate.
async fn estimate_agrees(depth: u64) -> anyhow::Result<()> {
    let devnet = Devnet::pinned().await?;
    let gas = devnet
        .agreed_estimate(call_message(recurse_calldata(depth), None).await?)
        .await?;
    eprintln!("depth={depth} block={:?} estimate={gas}", devnet.block);
    Ok(())
}

/// The estimate Forest returns must actually be enough to land the transaction.
async fn estimate_is_sufficient_on_chain() -> anyhow::Result<()> {
    // No cross-node comparison here, so `Latest` is fine: the estimate must reflect the same
    // fresh state the following `forest-wallet send` executes against.
    let latest = BlockNumberOrHash::PredefinedBlock(Predefined::Latest);
    let calldata = recurse_calldata(NESTED_DEPTH);
    let estimate = estimate(
        &forest_client()?,
        call_message(calldata.clone(), None).await?,
        latest,
    )
    .await?;
    // `forest-wallet send` infers `InvokeContract` and CBOR-wraps the params when the sender is an
    // eth account, and rejects an explicit `--method`, so pass the bare calldata.
    let cid = wallet_send_calldata(&sender().await?.cli, contract().await?, &calldata, estimate)
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

/// A call that reverts at any gas limit is reported as a revert, with or without a cap, never
/// answered with a gas value.
async fn estimate_reports_a_non_gas_failure() -> anyhow::Result<()> {
    let devnet = Devnet::pinned().await?;
    for gas in [None, Some(BLOCK_GAS_LIMIT)] {
        let msg = call_message(selector(ALWAYS_REVERTS_SIGNATURE), gas).await?;
        let (forest, lotus) = tokio::try_join!(
            estimate_error(&devnet.forest, msg.clone(), devnet.block.clone()),
            estimate_error(&devnet.lotus, msg.clone(), devnet.block.clone()),
        )?;
        for (node, err) in [("forest", &forest), ("lotus", &lotus)] {
            ensure!(
                err.code() == EXECUTION_REVERTED_CODE,
                "{node}: expected code {EXECUTION_REVERTED_CODE} with gas={gas:?}, got {}: {}",
                err.code(),
                err.message()
            );
            ensure!(
                err.message().contains(ALWAYS_REVERTS_REASON),
                "{node}: revert reason `{ALWAYS_REVERTS_REASON}` missing from `{}`",
                err.message()
            );
        }
        // The data field carries the ABI-encoded reason eth clients decode.
        let (forest_data, lotus_data) = (
            forest.data().map(|d| d.get()),
            lotus.data().map(|d| d.get()),
        );
        ensure!(
            forest_data.is_some() && forest_data == lotus_data,
            "revert data differs: forest={forest_data:?} lotus={lotus_data:?}"
        );
    }
    Ok(())
}

/// A caller's `gas` bounds the estimate: a spare cap leaves it alone, an executable cap replaces
/// the margin, and a cap below the need or the inclusion cost is an error.
async fn estimate_honors_gas_cap() -> anyhow::Result<()> {
    let devnet = Devnet::pinned().await?;
    let calldata = recurse_calldata(NESTED_DEPTH);
    let call = async |gas: Option<u64>| call_message(calldata.clone(), gas).await;
    let uncapped = devnet.agreed_estimate(call(None).await?).await?;

    let spare = devnet
        .agreed_estimate(call(Some(uncapped * 2)).await?)
        .await?;
    ensure!(
        spare == uncapped,
        "a cap above the need changed the estimate: capped={spare} uncapped={uncapped}"
    );

    // Below the estimate but above the need, the cap takes the margin's place and is returned as is.
    let tight = uncapped * 9 / 10;
    let capped = devnet.agreed_estimate(call(Some(tight)).await?).await?;
    ensure!(
        capped == tight,
        "an executable cap must replace the estimation margin: cap={tight} estimate={capped}"
    );
    devnet.expect_call_ok(call(Some(capped)).await?).await?;

    // Half the estimate is short of what the nesting needs.
    let half = uncapped / 2;
    devnet
        .expect_estimate_error(
            call(Some(half)).await?,
            TRANSACTION_REJECTED_CODE,
            &format!("out of gas: gas required exceeds: {half}"),
        )
        .await?;
    // Below the inclusion cost, preflight rejects the message before it runs.
    devnet
        .expect_estimate_error(
            call(Some(21_000)).await?,
            INVALID_INPUT_CODE,
            "gas required exceeds allowance (21000)",
        )
        .await
}

/// A revert that more gas fixes is searched past, with or without a cap; a cap below the
/// contract's `gasleft()` bound is out of gas.
async fn estimate_searches_past_gas_dependent_revert() -> anyhow::Result<()> {
    let devnet = Devnet::pinned().await?;
    let calldata = selector(REQUIRES_HIGH_GAS_SIGNATURE);
    for gas in [None, Some(BLOCK_GAS_LIMIT)] {
        let estimate = devnet
            .agreed_estimate(call_message(calldata.clone(), gas).await?)
            .await?;
        ensure!(
            estimate > REQUIRES_HIGH_GAS_THRESHOLD,
            "expected an estimate above {REQUIRES_HIGH_GAS_THRESHOLD} with gas={gas:?}, got {estimate}"
        );
        devnet
            .expect_call_ok(call_message(calldata.clone(), Some(estimate)).await?)
            .await?;
    }

    let cap = REQUIRES_HIGH_GAS_THRESHOLD / 2;
    devnet
        .expect_estimate_error(
            call_message(calldata, Some(cap)).await?,
            TRANSACTION_REJECTED_CODE,
            &format!("out of gas: gas required exceeds: {cap}"),
        )
        .await
}

/// Estimates run with zero fees, so a sender that cannot pay for the gas still gets one, with or
/// without a cap.
async fn estimate_ignores_sender_funds_without_price() -> anyhow::Result<()> {
    // Funded before the block is pinned, so both nodes have the sender at that block.
    let poor = new_funded(POOR_SENDER_FUND_AMT).await?.eth;
    let devnet = Devnet::pinned().await?;
    let calldata = recurse_calldata(NESTED_DEPTH);
    for gas in [None, Some(BLOCK_GAS_LIMIT)] {
        let msg = EthCallMessage {
            from: Some(poor),
            ..call_message(calldata.clone(), gas).await?
        };
        let estimate = devnet.agreed_estimate(msg.clone()).await?;
        ensure!(
            estimate < BLOCK_GAS_LIMIT,
            "the estimate {estimate} saturated instead of converging, gas={gas:?}"
        );
        devnet
            .expect_call_ok(EthCallMessage {
                gas: Some(EthUint64(estimate)),
                ..msg
            })
            .await?;
    }
    Ok(())
}

/// A `gasPrice` or `maxFeePerGas` limits the estimate to the gas the sender can pay for after
/// sending the value.
async fn estimate_price_limits_gas() -> anyhow::Result<()> {
    let devnet = Devnet::pinned().await?;
    let base = call_message(recurse_calldata(NESTED_DEPTH), None).await?;
    let uncapped = devnet.agreed_estimate(base.clone()).await?;
    let from = base.from.context("the call has no sender")?;
    let balance = TokenAmount::from(
        devnet
            .forest
            .call(EthGetBalance::request((from, devnet.block.clone()))?)
            .await?,
    );
    // The price at which the sender affords about `gas`, and the gas a price buys.
    let price_for = |gas: u64| balance.div_floor(gas);
    let allowance = |price: &TokenAmount| {
        u64::try_from(balance.div_floor(price.atto().clone()).atto())
            .map_or(BLOCK_GAS_LIMIT, |gas| gas.min(BLOCK_GAS_LIMIT))
    };
    let priced = |max_fee: TokenAmount| EthCallMessage {
        max_fee_per_gas: Some(max_fee.into()),
        ..base.clone()
    };

    // A price that buys more than the estimate leaves it alone.
    let affordable = devnet
        .agreed_estimate(priced(price_for(uncapped * 2)))
        .await?;
    ensure!(
        affordable == uncapped,
        "an affordable price changed the estimate: priced={affordable} unpriced={uncapped}"
    );

    // A price that buys less than the estimate but more than the need: the allowance takes the
    // margin's place.
    let tight = price_for(uncapped * 9 / 10);
    let limited = devnet.agreed_estimate(priced(tight.clone())).await?;
    ensure!(
        limited == allowance(&tight),
        "an executable allowance must replace the estimation margin: allowance={} estimate={limited}",
        allowance(&tight)
    );

    // A price that buys less than the need is out of gas at the allowance.
    let short = price_for(uncapped / 2);
    devnet
        .expect_estimate_error(
            priced(short.clone()),
            TRANSACTION_REJECTED_CODE,
            &format!("out of gas: gas required exceeds: {}", allowance(&short)),
        )
        .await?;

    // The value comes out of the balance before any gas does.
    let value = balance.clone() + TokenAmount::from_atto(1);
    devnet
        .expect_estimate_error(
            EthCallMessage {
                gas_price: Some(EthBigInt::from(1)),
                value: Some((&value).into()),
                ..base
            },
            TRANSACTION_REJECTED_CODE,
            &format!(
                "insufficient funds for gas * price + value: have {} want {}",
                balance.atto(),
                value.atto()
            ),
        )
        .await
}

/// Conflicting or negative prices are invalid params, rejected before the block is looked at.
async fn estimate_rejects_invalid_prices() -> anyhow::Result<()> {
    let devnet = Devnet::pinned().await?;
    let base = call_message(recurse_calldata(CONTROL_DEPTH), None).await?;

    let both_prices = EthCallMessage {
        gas_price: Some(EthBigInt::from(1)),
        max_fee_per_gas: Some(EthBigInt::from(1)),
        ..base.clone()
    };
    // Prices are checked before the block is resolved, so a block neither node has gets the same answer.
    let at_future_block = Devnet {
        forest: forest_client()?,
        lotus: lotus_client()?,
        block: BlockNumberOrHash::from_block_number(i64::from(u32::MAX)),
    };
    for nodes in [&devnet, &at_future_block] {
        nodes
            .expect_estimate_error(
                both_prices.clone(),
                INVALID_PARAMS_CODE,
                BOTH_PRICES_MESSAGE,
            )
            .await?;
    }

    // `EthBigInt` is unsigned, so the negative price goes into the raw request. Only the code is
    // compared: Lotus says `gas price must not be negative`, Forest reports its decode error.
    for field in ["gasPrice", "maxFeePerGas"] {
        let mut request = EthEstimateGas::request((base.clone(), Some(devnet.block.clone())))?;
        request
            .params
            .get_mut(0)
            .and_then(serde_json::Value::as_object_mut)
            .context("the call message is not a JSON object")?
            .insert(field.into(), "0x-1".into());
        for (node, client) in devnet.nodes() {
            let err = match client.call(request.clone()).await {
                Ok(gas) => bail!("{node}: returned {} for a negative {field}", gas.0),
                Err(e) => anyhow::Error::from(e),
            };
            let err = rpc_call_err(&err).with_context(|| {
                format!("{node}: expected a JSON-RPC error for a negative {field}: {err:?}")
            })?;
            ensure!(
                err.code() == INVALID_PARAMS_CODE,
                "{node}: expected code {INVALID_PARAMS_CODE} for a negative {field}, got code {} `{}`",
                err.code(),
                err.message()
            );
        }
    }
    Ok(())
}

/// The sender's balance is read from the state after the block's messages: a sender funded in the
/// very block the estimate runs at can pay for gas there.
async fn estimate_reads_balance_after_the_block() -> anyhow::Result<()> {
    let devnet = Devnet::pinned().await?;
    let wallet = new_unfunded().await?;
    let funding = send_from(
        &FOREST_TEST_PRELOADED_ADDRESS,
        &wallet.cli,
        "1 FIL",
        Backend::Local,
    )?;
    let cid = assert_send_ok(&funding).await?;
    // The lookup names the tipset that executed the message; its parent is the block that includes it.
    let executed = poll_until_message_executed(&devnet.forest, cid).await?;
    let executed = devnet
        .forest
        .call(ChainGetTipSet::request((ApiTipsetKey(Some(
            executed.tipset,
        )),))?)
        .await?;
    let included = devnet
        .forest
        .call(ChainGetTipSet::request((ApiTipsetKey(Some(
            executed.parents().clone(),
        )),))?)
        .await?;
    let in_block = BlockNumberOrHash::from_block_number(included.epoch());
    poll_until_actor_on("lotus", wallet.f4, lotus_client).await?;

    let msg = EthCallMessage {
        from: Some(wallet.eth),
        gas_price: Some(EthBigInt::from(1)),
        ..call_message(recurse_calldata(CONTROL_DEPTH), None).await?
    };
    for (node, client) in devnet.nodes() {
        let balance = client
            .call(EthGetBalance::request((wallet.eth, in_block.clone()))?)
            .await?;
        ensure!(
            !balance.is_zero(),
            "{node}: the sender must be funded at the block that includes the transfer"
        );
        estimate(client, msg.clone(), in_block.clone())
            .await
            .with_context(|| {
                format!("{node}: a sender funded in the block must pay for gas at that block")
            })?;
    }
    Ok(())
}

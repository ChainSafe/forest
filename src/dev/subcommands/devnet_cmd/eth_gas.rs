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
use crate::rpc::eth::errors::EXECUTION_REVERTED_CODE;
use crate::rpc::eth::{
    BlockNumberOrHash, Predefined,
    types::{EthAddress, EthBytes, EthCallMessage},
};
use crate::rpc::prelude::*;
use crate::shim::address::Address;
use crate::utils::encoding::{hex, keccak_256};
use anyhow::{Context as _, ensure};
use cid::Cid;
use jsonrpsee::core::ClientError;
use libtest_mimic::{Arguments, Failed, Trial};
use std::io::Write as _;
use std::str::FromStr as _;
use tempfile::NamedTempFile;
use tokio::sync::OnceCell;

/// `NestedGas`, whose `recurse(uint256)` calls itself that many times.
/// Regenerate with `contracts/compile.sh` after editing the source.
const NESTED_GAS_HEX: &str = include_str!("contracts/nested_gas/nested_gas.hex");
const RECURSE_SIGNATURE: &str = "recurse(uint256)";
/// Reverts explicitly unless given a large gas limit, so estimating it fails for a reason no
/// amount of extra gas can be shown to fix.
const REQUIRES_HIGH_GAS_SIGNATURE: &str = "requiresHighGasLimit()";
/// The `require` string in [`REQUIRES_HIGH_GAS_SIGNATURE`].
const REVERT_REASON: &str = "gas limit too low";
/// Both implementations prefix this branch's error with it. Asserting on it pins *which* rejection
/// happened: a message that failed earlier, during plain gas estimation, would never carry it.
const GAS_SEARCH_FAILURE: &str = "gas search failed";
const HEX_IN_CONTAINER: &str = "/tmp/nested_gas.hex";

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

/// Deployed `NestedGas` addresses: `eth` for the JSON-RPC calls, `f4` as the `lotus send` target.
struct Deployed {
    eth: EthAddress,
    f4: Address,
}

/// Deploys `NestedGas` once per process.
async fn contract() -> anyhow::Result<&'static Deployed> {
    static CONTRACT: OnceCell<Deployed> = OnceCell::const_new();
    CONTRACT
        .get_or_try_init(|| async {
            let mut hex_file = NamedTempFile::new_in(std::env::temp_dir())
                .context("staging the contract bytecode")?;
            hex_file.write_all(NESTED_GAS_HEX.trim().as_bytes())?;
            hex_file.flush()?;
            docker(&[
                "cp",
                &hex_file.path().to_string_lossy(),
                &format!("lotus:{HEX_IN_CONTAINER}"),
            ])?;

            let from = sender_addr().await?.to_string();
            let deploy = lotus_exec_retrying_transient(&[
                "evm",
                "deploy",
                "--from",
                &from,
                "--hex",
                HEX_IN_CONTAINER,
            ])
            .await?;
            let f4 = deploy
                .lines()
                .find_map(|l| l.trim().strip_prefix("f4 Address: "))
                .with_context(|| format!("no `f4 Address:` in deploy output:\n{deploy}"))?;
            let f4 = Address::from_str(f4.trim()).context("parsing the deployed f4 address")?;
            eprintln!("deployed NestedGas at {f4}");
            anyhow::Ok(Deployed {
                eth: EthAddress::from_filecoin_address(&f4)?,
                f4,
            })
        })
        .await
}

/// An `f4` sender funded well enough to afford the gas limits under test. Lotus rejects
/// estimation from an unfunded or non-`f4` sender, so both properties are required.
///
/// Created in Lotus's keystore rather than Forest's: estimation only needs the address to
/// exist on chain, while submitting messages and deploying the contract (both run on Lotus)
/// need whoever signs to hold the key.
async fn sender_addr() -> anyhow::Result<&'static Address> {
    static SENDER: OnceCell<Address> = OnceCell::const_new();
    SENDER
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
            let sender = Address::from_str(&addr).context("parsing the sender address")?;
            poll_until_actor_on("lotus", sender, lotus_client).await?;
            Ok(sender)
        })
        .await
}

async fn estimate(
    client: &Client,
    calldata: Vec<u8>,
    block: BlockNumberOrHash,
) -> anyhow::Result<u64> {
    let (sender, deployed) = tokio::try_join!(sender_addr(), contract())?;
    let msg = EthCallMessage {
        from: Some(EthAddress::from_filecoin_address(sender)?),
        to: Some(deployed.eth),
        data: Some(EthBytes(calldata)),
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

/// Deploy + fund, build both node clients, and pin a block height both have executed. Sampling the
/// height only after the deploy/fund guarantees the pinned tipset already contains the contract and
/// sender on both nodes (the funding poll also lets both catch up to the deploy).
async fn pinned_common_block() -> anyhow::Result<(Client, Client, i64)> {
    tokio::try_join!(contract(), sender_addr())?;
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
            )
            .await
            .context("EthEstimateGas on forest")
        },
        async {
            estimate(
                &lotus_c,
                recurse_calldata(depth),
                BlockNumberOrHash::from_block_number(block),
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
    // fresh state the following `lotus send` executes against.
    let estimate = estimate(
        &forest,
        recurse_calldata(NESTED_DEPTH),
        BlockNumberOrHash::PredefinedBlock(Predefined::Latest),
    )
    .await?;
    let sender = sender_addr().await?.to_string();
    let target = contract().await?.f4.to_string();
    let params = hex::encode(recurse_calldata(NESTED_DEPTH));
    let gas_limit = estimate.to_string();
    // `lotus send` infers `InvokeContract` and CBOR-wraps the params when the sender is an
    // eth account, and rejects an explicit `--method`, so pass the bare calldata. Retry the
    // submit while Lotus's mpool briefly lags the freshly funded sender.
    let out = lotus_exec_retrying_transient(&[
        "send",
        "--from",
        &sender,
        "--params-hex",
        &params,
        "--gas-limit",
        &gas_limit,
        &target,
        "0",
    ])
    .await?;
    let cid = out
        .lines()
        .last()
        .context("no cid from `lotus send`")?
        .trim();
    eprintln!("submitted at forest's estimate {estimate}: {cid}");

    let lookup = forest
        .call(
            StateWaitMsg::request((Cid::from_str(cid)?, 0, 800, true))?.with_timeout(POLL_TIMEOUT),
        )
        .await?;
    let exit = lookup.receipt.exit_code();
    ensure!(
        exit.is_success(),
        "a transaction submitted at forest's own eth_estimateGas value ({estimate}) failed \
         on chain with exit code {exit}; the estimate is not a usable gas limit"
    );
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
        // Cross-node parity: both name the branch ("gas search failed") and the decoded revert reason.
        ensure!(
            obj.message().contains(GAS_SEARCH_FAILURE),
            "{node} rejected the call before the gas search, so this no longer exercises the \
             branch it is meant to pin (expected `{GAS_SEARCH_FAILURE}`): {}",
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

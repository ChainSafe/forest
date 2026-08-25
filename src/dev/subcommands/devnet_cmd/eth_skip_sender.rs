// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Skip-sender `eth_call` and `eth_estimateGas` tests on the docker devnet
//! (`scripts/devnet`). These cases need a private chain: deploy a contract, fund
//! an address, submit a transaction, or assert Forest state after a skip-call.
//!
//! Tests deploy `SimpleCoin`, `ContractA` / `ContractB`, `NestedGas`, or `Errors`
//! as needed. They cover estimate-then-submit from an unfunded `from` (including
//! nested `recurse`), estimate parity with a funded placeholder, `msg.sender`
//! identity via `sendCoin`, skip-call state isolation, a historical `eth_call`,
//! cross-contract callbacks, and the skip-sender success/error matrix (CREATE,
//! `gasPrice`, `FromNil`, `FromEOA`, value, revert data).

use crate::dev::subcommands::tests_cmd::helpers::*;
use crate::rpc::Client;
use crate::rpc::eth::errors::EXECUTION_REVERTED_CODE;
use crate::rpc::eth::{
    BlockNumberOrHash, EthBigInt, Predefined,
    types::{EthAddress, EthBytes, EthCallMessage},
};
use crate::rpc::prelude::*;
use crate::rpc::types::ApiTipsetKey;
use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;
use crate::shim::state_tree::ActorState;
use crate::utils::encoding::{hex, keccak_256};
use anyhow::{Context as _, ensure};
use cid::Cid;
use jsonrpsee::core::ClientError;
use libtest_mimic::{Arguments, Failed, Trial};
use std::io::Write as _;
use std::str::FromStr as _;
use tempfile::NamedTempFile;
use tokio::sync::OnceCell;

const SIMPLE_COIN_HEX: &str = include_str!("contracts/simple_coin/simple_coin.hex");
const CONTRACT_A_HEX: &str = include_str!("contracts/contract_a/contract_a.hex");
const CONTRACT_B_HEX: &str = include_str!("contracts/contract_b/contract_b.hex");
const NESTED_GAS_HEX: &str = include_str!("contracts/nested_gas/nested_gas.hex");
const ERRORS_HEX: &str = include_str!("contracts/errors/errors.hex");

const SEND_COIN_SIGNATURE: &str = "sendCoin(address,uint256)";
const SET_CONTRACT_B_SIGNATURE: &str = "setContractB(address)";
const GET_BALANCE_SIGNATURE: &str = "getBalance(address)";
const CALL_B_AND_READ_BACK: &str = "callBAndReadBack()";
const CALL_B_AND_DOUBLE: &str = "callBAndDouble()";
const RECURSE_SIGNATURE: &str = "recurse(uint256)";
const FAIL_DIV_ZERO: &str = "failDivZero()";
const FAIL_ASSERT: &str = "failAssert()";
const FAIL_REVERT_REASON: &str = "failRevertReason()";
const FAIL_REVERT_EMPTY: &str = "failRevertEmpty()";
const FAIL_CUSTOM: &str = "failCustom()";

const NESTED_DEPTH: u64 = 100;
const DEPLOYER_FUND_AMT: &str = "10 FIL";
const ROUND_TRIP_FUND_AMT: &str = "1 FIL";
const RECURSIVE_FUND_AMT: &str = "10 FIL";
const EOA_FUND_AMT: &str = "10 FIL";
const PLACEHOLDER_FUND_AMT: &str = "2 FIL";
const ESTIMATE_PARITY: f64 = 0.10;
const GAS_PRICE: u64 = 1_000_000_000;
const MIN_ESTIMATE_GAS: u64 = 21_000;
const MAX_ESTIMATE_GAS: u64 = 10_000_000_000;
/// ABI `Panic(uint256)` payload for Solidity assert (`0x01`) and division by zero (`0x12`).
const PANIC_ASSERT: &str =
    "4e487b710000000000000000000000000000000000000000000000000000000000000001";
const PANIC_DIV_ZERO: &str =
    "4e487b710000000000000000000000000000000000000000000000000000000000000012";

/// Skip-sender integration tests that need a private chain with a miner
#[derive(Debug, clap::Args)]
pub struct EthSkipSenderTestCommand {}

impl EthSkipSenderTestCommand {
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
        trial("round_trip_from_unfunded", || {
            block_on(round_trip_from_unfunded())
        }),
        trial("parity_with_existing_sender", || {
            block_on(parity_with_existing_sender())
        }),
        trial("round_trip_recursive", || block_on(round_trip_recursive())),
        trial("call_sender_identity", || block_on(call_sender_identity())),
        trial("skip_sender_state_isolation", || {
            block_on(skip_sender_state_isolation())
        }),
        trial("skip_sender_historical_call", || {
            block_on(skip_sender_historical_call())
        }),
        trial("cross_contract_from_contract", || {
            block_on(cross_contract_from_contract())
        }),
        trial("cross_contract_from_missing", || {
            block_on(cross_contract_from_missing())
        }),
        trial("cross_contract_from_eoa", || {
            block_on(cross_contract_from_eoa())
        }),
        trial("cross_contract_double_callback", || {
            block_on(cross_contract_double_callback())
        }),
        trial("call_skip_sender", || block_on(call_skip_sender())),
        trial("estimate_gas_skip_sender", || {
            block_on(estimate_gas_skip_sender())
        }),
        trial("funded_placeholder_sender", || {
            block_on(funded_placeholder_sender())
        }),
    ]
}

fn selector(signature: &str) -> Vec<u8> {
    keccak_256(signature.as_bytes())
        .get(..4)
        .expect("keccak256 is 32 bytes")
        .to_vec()
}

fn abi_address_word(addr: EthAddress) -> Vec<u8> {
    let mut word = vec![0u8; 12];
    word.extend_from_slice(addr.0.as_bytes());
    word
}

fn send_coin_calldata(to: EthAddress, amount: u64) -> Vec<u8> {
    let mut out = selector(SEND_COIN_SIGNATURE);
    out.extend(abi_address_word(to));
    out.extend_from_slice(&ethereum_types::U256::from(amount).to_big_endian());
    out
}

fn set_contract_b_calldata(addr: EthAddress) -> Vec<u8> {
    let mut out = selector(SET_CONTRACT_B_SIGNATURE);
    out.extend(abi_address_word(addr));
    out
}

fn recurse_calldata(depth: u64) -> Vec<u8> {
    let mut out = selector(RECURSE_SIGNATURE);
    out.extend_from_slice(&ethereum_types::U256::from(depth).to_big_endian());
    out
}

fn get_balance_calldata(addr: EthAddress) -> Vec<u8> {
    let mut out = selector(GET_BALANCE_SIGNATURE);
    out.extend(abi_address_word(addr));
    out
}

fn simple_coin_initcode() -> anyhow::Result<EthBytes> {
    Ok(EthBytes(
        hex::decode(SIMPLE_COIN_HEX.trim()).context("decoding SimpleCoin initcode")?,
    ))
}

/// Missing eth address: `0xdeadbeef` then zeros, last byte `seed`.
fn non_existent(seed: u8) -> anyhow::Result<EthAddress> {
    EthAddress::from_str(&format!("0xdeadbeef{:030}{seed:02x}", 0))
        .context("parsing missing eth address")
}

fn latest() -> BlockNumberOrHash {
    BlockNumberOrHash::PredefinedBlock(Predefined::Latest)
}

/// Deployed EVM actor: `eth` for JSON-RPC, `f4` for `lotus send` / `StateGetActor`.
#[derive(Clone, Copy)]
struct Deployed {
    eth: EthAddress,
    f4: Address,
}

/// Dedicated delegated wallet used to deploy and to credit `SimpleCoin`.
/// Not the genesis/miner key: that wallet races with the miner on nonce.
async fn deployer() -> anyhow::Result<&'static Address> {
    static DEPLOYER: OnceCell<Address> = OnceCell::const_new();
    DEPLOYER
        .get_or_try_init(|| async {
            let addr = lotus_exec(&["wallet", "new", "delegated"])?;
            let msg = send_from(
                &FOREST_TEST_PRELOADED_ADDRESS,
                &addr,
                DEPLOYER_FUND_AMT,
                Backend::Local,
            )?;
            eprintln!("funding deployer {addr} with {DEPLOYER_FUND_AMT}, msg: {msg}");
            let balance = poll_until_funded(&addr, Backend::Local).await?;
            eprintln!("deployer {addr} funded balance: {balance}");
            Address::from_str(&addr).context("parsing the deployer address")
        })
        .await
}

async fn deploy_hex(label: &str, bytecode: &str, container_path: &str) -> anyhow::Result<Deployed> {
    let deployer = deployer().await?;
    let mut hex_file =
        NamedTempFile::new_in(std::env::temp_dir()).context("staging the contract bytecode")?;
    hex_file.write_all(bytecode.trim().as_bytes())?;
    hex_file.flush()?;
    docker(&[
        "cp",
        &hex_file.path().to_string_lossy(),
        &format!("lotus:{container_path}"),
    ])?;

    let from = deployer.to_string();
    let deploy =
        lotus_exec_retrying_mpool(&["evm", "deploy", "--from", &from, "--hex", container_path])
            .await?;
    let f4 = deploy
        .lines()
        .find_map(|l| l.trim().strip_prefix("f4 Address: "))
        .with_context(|| format!("no `f4 Address:` in {label} deploy output:\n{deploy}"))?;
    let f4 = Address::from_str(f4.trim()).context("parsing the deployed f4 address")?;
    eprintln!("deployed {label} at {f4}");
    poll_until_actor(f4).await?;
    Ok(Deployed {
        eth: EthAddress::from_filecoin_address(&f4)?,
        f4,
    })
}

async fn simple_coin() -> anyhow::Result<&'static Deployed> {
    static CONTRACT: OnceCell<Deployed> = OnceCell::const_new();
    CONTRACT
        .get_or_try_init(|| deploy_hex("SimpleCoin", SIMPLE_COIN_HEX, "/tmp/simple_coin.hex"))
        .await
}

async fn contract_b() -> anyhow::Result<&'static Deployed> {
    static CONTRACT: OnceCell<Deployed> = OnceCell::const_new();
    CONTRACT
        .get_or_try_init(|| deploy_hex("ContractB", CONTRACT_B_HEX, "/tmp/contract_b.hex"))
        .await
}

async fn nested_gas() -> anyhow::Result<&'static Deployed> {
    static CONTRACT: OnceCell<Deployed> = OnceCell::const_new();
    CONTRACT
        .get_or_try_init(|| deploy_hex("NestedGas", NESTED_GAS_HEX, "/tmp/nested_gas_skip.hex"))
        .await
}

async fn errors_contract() -> anyhow::Result<&'static Deployed> {
    static CONTRACT: OnceCell<Deployed> = OnceCell::const_new();
    CONTRACT
        .get_or_try_init(|| deploy_hex("Errors", ERRORS_HEX, "/tmp/errors.hex"))
        .await
}

/// Shared senders and contracts for the skip-sender call/estimate tables.
struct TableEnv {
    coin: EthAddress,
    errors: EthAddress,
    eoa: EthAddress,
    eoa2: EthAddress,
}

async fn table_env() -> anyhow::Result<&'static TableEnv> {
    static ENV: OnceCell<TableEnv> = OnceCell::const_new();
    ENV.get_or_try_init(|| async {
        let coin = simple_coin().await?;
        let errors = errors_contract().await?;
        let (_eoa_f4, eoa) = new_funded(EOA_FUND_AMT).await?;
        let (_eoa2_f4, eoa2) = new_unfunded().await?;
        Ok(TableEnv {
            coin: coin.eth,
            errors: errors.eth,
            eoa,
            eoa2,
        })
    })
    .await
}

/// `ContractA` with `setContractB` already mined, so callbacks see `storedValue`.
async fn linked_contracts() -> anyhow::Result<&'static (Deployed, Deployed)> {
    static LINKED: OnceCell<(Deployed, Deployed)> = OnceCell::const_new();
    LINKED
        .get_or_try_init(|| async {
            let b = contract_b().await?;
            let a = deploy_hex("ContractA", CONTRACT_A_HEX, "/tmp/contract_a.hex").await?;
            invoke(&a.f4, &set_contract_b_calldata(b.eth)).await?;
            Ok((
                a,
                Deployed {
                    eth: b.eth,
                    f4: b.f4,
                },
            ))
        })
        .await
}

async fn get_actor(client: &Client, addr: Address) -> anyhow::Result<Option<ActorState>> {
    client
        .call(StateGetActor::request((addr, ApiTipsetKey(None)))?)
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))
}

async fn poll_until_actor(addr: Address) -> anyhow::Result<ActorState> {
    let client = forest_client()?;
    let started = tokio::time::Instant::now();
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        eprintln!("Polling StateGetActor {addr} attempt {attempt}");
        if let Some(actor) = get_actor(&client, addr).await? {
            return Ok(actor);
        }
        if started.elapsed() >= POLL_TIMEOUT {
            anyhow::bail!("Timed out waiting for actor {addr} after {POLL_TIMEOUT:?}");
        }
        let remaining = POLL_TIMEOUT.saturating_sub(started.elapsed());
        tokio::time::sleep(POLL_WAIT_TIME.min(remaining)).await;
    }
}

async fn wait_for_cid(forest: &Client, cid: Cid) -> anyhow::Result<()> {
    let lookup = forest
        .call(StateWaitMsg::request((cid, 0, 800, true))?.with_timeout(POLL_TIMEOUT))
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    let exit = lookup.receipt.exit_code();
    ensure!(
        exit.is_success(),
        "message {cid} failed on chain with exit code {exit}"
    );
    Ok(())
}

async fn invoke(to: &Address, calldata: &[u8]) -> anyhow::Result<Cid> {
    let forest = forest_client()?;
    let from = deployer().await?.to_string();
    let to_s = to.to_string();
    let params = hex::encode(calldata);
    let out =
        lotus_exec_retrying_mpool(&["send", "--from", &from, "--params-hex", &params, &to_s, "0"])
            .await?;
    let cid = Cid::from_str(
        out.lines()
            .last()
            .context("no cid from `lotus send`")?
            .trim(),
    )?;
    wait_for_cid(&forest, cid).await?;
    Ok(cid)
}

async fn submit_at_gas_limit(
    from: &Address,
    to: &Address,
    calldata: &[u8],
    gas_limit: u64,
) -> anyhow::Result<()> {
    let forest = forest_client()?;
    let from_s = from.to_string();
    let to_s = to.to_string();
    let params = hex::encode(calldata);
    let gas = gas_limit.to_string();
    let out = lotus_exec_retrying_mpool(&[
        "send",
        "--from",
        &from_s,
        "--params-hex",
        &params,
        "--gas-limit",
        &gas,
        &to_s,
        "0",
    ])
    .await?;
    let cid = Cid::from_str(
        out.lines()
            .last()
            .context("no cid from `lotus send`")?
            .trim(),
    )?;
    eprintln!("submitted at estimate {gas_limit}: {cid}");
    wait_for_cid(&forest, cid).await?;
    let hash = forest
        .call(EthGetTransactionHashByCid::request((cid,))?)
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))?
        .context("no Ethereum transaction hash for submitted CID")?;
    let receipt = forest
        .call(EthGetTransactionReceipt::request((hash,))?)
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))?
        .context("no Ethereum receipt for submitted transaction")?;
    // `EthTxReceipt` fields are module-private; read `status` through serde.
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ReceiptStatus {
        status: crate::rpc::eth::EthUint64,
    }
    let status = serde_json::from_value::<ReceiptStatus>(serde_json::to_value(&receipt)?)?
        .status
        .0;
    ensure!(
        status == 1,
        "estimated gas limit {gas_limit} must be sufficient; receipt status is {status}"
    );
    Ok(())
}

async fn eth_call_msg(
    client: &Client,
    msg: EthCallMessage,
    block: BlockNumberOrHash,
) -> anyhow::Result<EthBytes> {
    Ok(client.call(EthCall::request((msg, block))?).await?)
}

async fn estimate_msg(client: &Client, msg: EthCallMessage) -> anyhow::Result<u64> {
    Ok(client
        .call(EthEstimateGas::request((msg, Some(latest())))?)
        .await?
        .0)
}

fn rpc_call_err(err: &anyhow::Error) -> Option<&jsonrpsee::types::ErrorObjectOwned> {
    match err.downcast_ref::<ClientError>() {
        Some(ClientError::Call(obj)) => Some(obj),
        _ => None,
    }
}

fn rpc_data(obj: &jsonrpsee::types::ErrorObjectOwned) -> Option<String> {
    let raw = obj.data()?;
    serde_json::from_str::<String>(raw.get())
        .ok()
        .or_else(|| Some(raw.get().trim_matches('"').to_string()))
}

enum Expect {
    Success,
    SuccessGas,
    ErrContains(&'static str),
    Reverted {
        msg: &'static str,
        data_contains: Option<String>,
        data_eq: Option<&'static str>,
    },
}

struct SkipSenderCase {
    name: &'static str,
    msg: EthCallMessage,
    call: Option<Expect>,
    estimate: Option<Expect>,
}

fn fil(whole: u64) -> EthBigInt {
    EthBigInt::from(TokenAmount::from_whole(whole))
}

fn skip_sender_cases(env: &TableEnv) -> anyhow::Result<Vec<SkipSenderCase>> {
    let initcode = simple_coin_initcode()?;
    let missing = non_existent(0x01)?;
    let gas_price = Some(EthBigInt::from(GAS_PRICE));
    let custom = hex::encode(selector("CustomError()"));

    let transfer = |from: Option<EthAddress>, to: Option<EthAddress>| EthCallMessage {
        from,
        to,
        ..Default::default()
    };

    Ok(vec![
        SkipSenderCase {
            name: "CreateFromContract",
            msg: EthCallMessage {
                from: Some(env.coin),
                to: None,
                data: Some(initcode.clone()),
                ..Default::default()
            },
            call: Some(Expect::ErrContains("disallowed caller")),
            estimate: Some(Expect::ErrContains("disallowed caller")),
        },
        SkipSenderCase {
            name: "CreateFromNonExistent",
            msg: EthCallMessage {
                from: Some(missing),
                to: None,
                data: Some(initcode),
                ..Default::default()
            },
            call: Some(Expect::Success),
            estimate: Some(Expect::SuccessGas),
        },
        SkipSenderCase {
            name: "FromContract",
            msg: transfer(Some(env.coin), Some(env.eoa)),
            call: Some(Expect::Success),
            estimate: Some(Expect::SuccessGas),
        },
        SkipSenderCase {
            name: "FromContractWithGasPrice",
            msg: EthCallMessage {
                from: Some(env.coin),
                to: Some(env.eoa),
                gas_price,
                ..Default::default()
            },
            call: Some(Expect::Success),
            estimate: Some(Expect::SuccessGas),
        },
        SkipSenderCase {
            name: "FromContractToSelf",
            msg: EthCallMessage {
                from: Some(env.coin),
                to: Some(env.coin),
                data: Some(EthBytes(get_balance_calldata(env.coin))),
                ..Default::default()
            },
            call: Some(Expect::Success),
            estimate: Some(Expect::SuccessGas),
        },
        SkipSenderCase {
            name: "FromContractWithValue",
            msg: EthCallMessage {
                from: Some(env.coin),
                to: Some(env.eoa),
                value: Some(fil(1)),
                ..Default::default()
            },
            call: Some(Expect::ErrContains("insufficient")),
            estimate: Some(Expect::ErrContains("insufficient")),
        },
        SkipSenderCase {
            name: "FromNonExistent",
            msg: transfer(Some(missing), Some(env.eoa)),
            call: Some(Expect::Success),
            estimate: Some(Expect::SuccessGas),
        },
        SkipSenderCase {
            name: "FromNonExistentWithGasPrice",
            msg: EthCallMessage {
                from: Some(missing),
                to: Some(env.eoa),
                gas_price,
                ..Default::default()
            },
            call: Some(Expect::Success),
            estimate: Some(Expect::SuccessGas),
        },
        SkipSenderCase {
            name: "FromNonExistentToContractWithData",
            msg: EthCallMessage {
                from: Some(missing),
                to: Some(env.errors),
                data: Some(EthBytes(selector(FAIL_REVERT_EMPTY))),
                ..Default::default()
            },
            call: Some(Expect::Reverted {
                msg: "none",
                data_contains: None,
                data_eq: Some("0x"),
            }),
            estimate: Some(Expect::Reverted {
                msg: "none",
                data_contains: None,
                data_eq: Some("0x"),
            }),
        },
        SkipSenderCase {
            name: "FromNonExistentWithValue",
            msg: EthCallMessage {
                from: Some(missing),
                to: Some(env.eoa),
                value: Some(fil(1)),
                ..Default::default()
            },
            call: Some(Expect::ErrContains("insufficient")),
            estimate: None,
        },
        SkipSenderCase {
            name: "FromEOA",
            msg: transfer(Some(env.eoa), Some(env.eoa2)),
            call: Some(Expect::Success),
            estimate: Some(Expect::SuccessGas),
        },
        SkipSenderCase {
            name: "FromNil",
            msg: transfer(None, Some(env.eoa)),
            call: Some(Expect::Success),
            estimate: None,
        },
        SkipSenderCase {
            name: "ValueOverBalance",
            msg: EthCallMessage {
                from: Some(env.eoa),
                to: Some(missing),
                value: Some(fil(11)),
                ..Default::default()
            },
            call: Some(Expect::ErrContains("insufficient")),
            estimate: None,
        },
        SkipSenderCase {
            name: "RevertDivideByZero",
            msg: EthCallMessage {
                from: Some(env.eoa),
                to: Some(env.errors),
                data: Some(EthBytes(selector(FAIL_DIV_ZERO))),
                ..Default::default()
            },
            call: Some(Expect::Reverted {
                msg: "DivideByZero",
                data_contains: Some(PANIC_DIV_ZERO.to_string()),
                data_eq: None,
            }),
            estimate: Some(Expect::Reverted {
                msg: "DivideByZero",
                data_contains: Some(PANIC_DIV_ZERO.to_string()),
                data_eq: None,
            }),
        },
        SkipSenderCase {
            name: "RevertAssert",
            msg: EthCallMessage {
                from: Some(env.eoa),
                to: Some(env.errors),
                data: Some(EthBytes(selector(FAIL_ASSERT))),
                ..Default::default()
            },
            call: Some(Expect::Reverted {
                msg: "Assert",
                data_contains: Some(PANIC_ASSERT.to_string()),
                data_eq: None,
            }),
            estimate: Some(Expect::Reverted {
                msg: "Assert",
                data_contains: Some(PANIC_ASSERT.to_string()),
                data_eq: None,
            }),
        },
        SkipSenderCase {
            name: "RevertWithReason",
            msg: EthCallMessage {
                from: Some(env.eoa),
                to: Some(env.errors),
                data: Some(EthBytes(selector(FAIL_REVERT_REASON))),
                ..Default::default()
            },
            call: Some(Expect::Reverted {
                msg: "my reason",
                data_contains: None,
                data_eq: None,
            }),
            estimate: Some(Expect::Reverted {
                msg: "my reason",
                data_contains: None,
                data_eq: None,
            }),
        },
        SkipSenderCase {
            name: "RevertEmpty",
            msg: EthCallMessage {
                from: Some(env.eoa),
                to: Some(env.errors),
                data: Some(EthBytes(selector(FAIL_REVERT_EMPTY))),
                ..Default::default()
            },
            call: Some(Expect::Reverted {
                msg: "none",
                data_contains: None,
                data_eq: Some("0x"),
            }),
            estimate: Some(Expect::Reverted {
                msg: "none",
                data_contains: None,
                data_eq: Some("0x"),
            }),
        },
        SkipSenderCase {
            name: "RevertCustomError",
            msg: EthCallMessage {
                from: Some(env.eoa),
                to: Some(env.errors),
                data: Some(EthBytes(selector(FAIL_CUSTOM))),
                ..Default::default()
            },
            call: Some(Expect::Reverted {
                msg: "",
                data_contains: Some(custom.clone()),
                data_eq: None,
            }),
            estimate: Some(Expect::Reverted {
                msg: "",
                data_contains: Some(custom),
                data_eq: None,
            }),
        },
    ])
}

fn assert_expect(
    label: &str,
    result: Result<u64, anyhow::Error>,
    expect: &Expect,
) -> anyhow::Result<()> {
    match expect {
        Expect::Success => {
            result.with_context(|| format!("{label}: expected success"))?;
            Ok(())
        }
        Expect::SuccessGas => {
            let gas = result.with_context(|| format!("{label}: expected a gas estimate"))?;
            ensure!(
                gas >= MIN_ESTIMATE_GAS,
                "{label}: estimate {gas} is below the 21_000 transfer floor"
            );
            ensure!(
                gas < MAX_ESTIMATE_GAS,
                "{label}: estimate {gas} looks like an overflow"
            );
            Ok(())
        }
        Expect::ErrContains(needle) => {
            let err = result.err().with_context(|| {
                format!("{label}: expected an error containing `{needle}`, but the call succeeded")
            })?;
            let text = match rpc_call_err(&err) {
                Some(obj) => {
                    let mut s = obj.message().to_string();
                    if let Some(data) = rpc_data(obj) {
                        s.push(' ');
                        s.push_str(&data);
                    }
                    s
                }
                None => err.to_string(),
            };
            ensure!(
                text.to_ascii_lowercase()
                    .contains(&needle.to_ascii_lowercase()),
                "{label}: error `{text}` does not contain `{needle}`"
            );
            Ok(())
        }
        Expect::Reverted {
            msg,
            data_contains,
            data_eq,
        } => {
            let err = result.err().with_context(|| {
                format!("{label}: expected execution reverted, but the call succeeded")
            })?;
            let obj = rpc_call_err(&err)
                .with_context(|| format!("{label}: expected a JSON-RPC error, got {err:#}"))?;
            ensure!(
                obj.code() == EXECUTION_REVERTED_CODE,
                "{label}: expected execution-reverted code {EXECUTION_REVERTED_CODE}, got {}: {}",
                obj.code(),
                obj.message()
            );
            if !msg.is_empty() {
                ensure!(
                    obj.message().contains(msg),
                    "{label}: revert message `{}` does not contain `{msg}`",
                    obj.message()
                );
            }
            let data = rpc_data(obj).unwrap_or_default();
            if let Some(want) = data_eq {
                ensure!(data == *want, "{label}: revert data `{data}` != `{want}`");
            }
            if let Some(want) = data_contains {
                ensure!(
                    data.contains(want),
                    "{label}: revert data `{data}` does not contain `{want}`"
                );
            }
            Ok(())
        }
    }
}

async fn call_skip_sender() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let env = table_env().await?;
    for case in skip_sender_cases(env)? {
        let Some(expect) = case.call else {
            continue;
        };
        let label = format!("eth_call {}", case.name);
        let result = eth_call_msg(&forest, case.msg, latest())
            .await
            .map(|_| 0u64);
        assert_expect(&label, result, &expect)?;
    }
    Ok(())
}

async fn estimate_gas_skip_sender() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let env = table_env().await?;
    for case in skip_sender_cases(env)? {
        let Some(expect) = case.estimate else {
            continue;
        };
        let label = format!("eth_estimateGas {}", case.name);
        assert_expect(&label, estimate_msg(&forest, case.msg).await, &expect)?;
    }
    Ok(())
}

async fn funded_placeholder_sender() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let (_from_f4, from) = new_funded(PLACEHOLDER_FUND_AMT).await?;
    let (_to_f4, to) = new_funded(ROUND_TRIP_FUND_AMT).await?;
    eth_call_msg(
        &forest,
        EthCallMessage {
            from: Some(from),
            to: Some(to),
            value: Some(fil(1)),
            ..Default::default()
        },
        latest(),
    )
    .await
    .context("value-bearing eth_call from a funded placeholder")?;
    Ok(())
}

async fn estimate_gas(
    client: &Client,
    from: EthAddress,
    to: EthAddress,
    data: Vec<u8>,
) -> anyhow::Result<u64> {
    estimate_msg(
        client,
        EthCallMessage {
            from: Some(from),
            to: Some(to),
            data: Some(EthBytes(data)),
            ..Default::default()
        },
    )
    .await
}

async fn eth_call(
    client: &Client,
    from: EthAddress,
    to: EthAddress,
    data: Vec<u8>,
    block: BlockNumberOrHash,
) -> anyhow::Result<EthBytes> {
    let msg = EthCallMessage {
        from: Some(from),
        to: Some(to),
        data: (!data.is_empty()).then_some(EthBytes(data)),
        ..Default::default()
    };
    client
        .call(EthCall::request((msg, block))?)
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))
}

fn within_parity(skip: u64, funded: u64) -> bool {
    let denom = funded.max(1) as f64;
    (skip as f64 - funded as f64).abs() / denom <= ESTIMATE_PARITY
}

async fn new_unfunded() -> anyhow::Result<(Address, EthAddress)> {
    let addr = lotus_exec(&["wallet", "new", "delegated"])?;
    let f4 = Address::from_str(&addr).context("parsing unfunded delegated address")?;
    let eth = EthAddress::from_filecoin_address(&f4)?;
    Ok((f4, eth))
}

async fn new_funded(amount: &str) -> anyhow::Result<(Address, EthAddress)> {
    let (f4, eth) = new_unfunded().await?;
    let msg = send_from(
        &FOREST_TEST_PRELOADED_ADDRESS,
        &f4.to_string(),
        amount,
        Backend::Local,
    )?;
    eprintln!("funding placeholder {f4} with {amount}, msg: {msg}");
    poll_until_funded(&f4.to_string(), Backend::Local).await?;
    Ok((f4, eth))
}

async fn round_trip_from_unfunded() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let coin = simple_coin().await?;
    let recipient = non_existent(0x01)?;
    let calldata = send_coin_calldata(recipient, 0);

    let (from_f4, from_eth) = new_unfunded().await?;
    let gas = estimate_gas(&forest, from_eth, coin.eth, calldata.clone())
        .await
        .context("eth_estimateGas from unfunded sender")?;
    eprintln!("skip-sender estimate {gas} from {from_f4}");

    ensure!(
        get_actor(&forest, from_f4).await?.is_none(),
        "ephemeral placeholder for {from_f4} leaked onto chain during estimate"
    );

    let fund_msg = send_from(
        &FOREST_TEST_PRELOADED_ADDRESS,
        &from_f4.to_string(),
        ROUND_TRIP_FUND_AMT,
        Backend::Local,
    )?;
    eprintln!("funding round-trip sender {from_f4}, msg: {fund_msg}");
    poll_until_funded(&from_f4.to_string(), Backend::Local).await?;
    let actor = poll_until_actor(from_f4).await?;
    ensure!(
        actor.sequence == 0,
        "pre-submit nonce of {from_f4} is {}, expected 0 (placeholder must not have incremented it)",
        actor.sequence
    );

    submit_at_gas_limit(&from_f4, &coin.f4, &calldata, gas).await?;
    let after = get_actor(&forest, from_f4)
        .await?
        .with_context(|| format!("actor {from_f4} missing after successful submit"))?;
    ensure!(
        after.sequence == 1,
        "successful tx must use nonce 0; on-chain nonce is now {}",
        after.sequence
    );
    Ok(())
}

async fn parity_with_existing_sender() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let coin = simple_coin().await?;
    let calldata = send_coin_calldata(non_existent(0x01)?, 0);

    let skip = estimate_gas(&forest, non_existent(0x42)?, coin.eth, calldata.clone())
        .await
        .context("eth_estimateGas from missing from")?;
    let (_placeholder, placeholder_eth) = new_funded(ROUND_TRIP_FUND_AMT).await?;
    let funded = estimate_gas(&forest, placeholder_eth, coin.eth, calldata)
        .await
        .context("eth_estimateGas from funded placeholder")?;
    eprintln!("parity skip={skip} funded={funded}");
    ensure!(
        within_parity(skip, funded),
        "skip-sender estimate {skip} vs funded-placeholder {funded} exceeds 10%"
    );
    Ok(())
}

async fn round_trip_recursive() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let nested = nested_gas().await?;
    let calldata = recurse_calldata(NESTED_DEPTH);

    let (from_f4, from_eth) = new_unfunded().await?;
    let gas = estimate_gas(&forest, from_eth, nested.eth, calldata.clone())
        .await
        .context("skip-sender eth_estimateGas recurse(100)")?;

    let (_placeholder, placeholder_eth) = new_funded(RECURSIVE_FUND_AMT).await?;
    let funded = estimate_gas(&forest, placeholder_eth, nested.eth, calldata.clone())
        .await
        .context("funded-placeholder eth_estimateGas recurse(100)")?;
    eprintln!("recursive skip={gas} funded={funded}");
    ensure!(
        within_parity(gas, funded),
        "recursive skip-sender estimate {gas} vs funded-placeholder {funded} exceeds 10%"
    );

    let fund_msg = send_from(
        &FOREST_TEST_PRELOADED_ADDRESS,
        &from_f4.to_string(),
        RECURSIVE_FUND_AMT,
        Backend::Local,
    )?;
    eprintln!("funding recursive sender {from_f4}, msg: {fund_msg}");
    poll_until_funded(&from_f4.to_string(), Backend::Local).await?;
    submit_at_gas_limit(&from_f4, &nested.f4, &calldata, gas).await
}

async fn call_sender_identity() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let coin = simple_coin().await?;
    let sender_contract = contract_b().await?;
    let with_coins = non_existent(0x21)?;
    let without_coins = non_existent(0x22)?;
    let recipient = non_existent(0x01)?;

    for to in [sender_contract.eth, with_coins] {
        invoke(&coin.f4, &send_coin_calldata(to, 100)).await?;
    }

    let spend = send_coin_calldata(recipient, 10);
    for (label, from, want) in [
        ("contract from", sender_contract.eth, 1u8),
        ("credited missing from", with_coins, 1),
        ("uncounted missing from", without_coins, 0),
    ] {
        let ret = eth_call(&forest, from, coin.eth, spend.clone(), latest())
            .await
            .with_context(|| format!("eth_call sendCoin from {label}"))?;
        ensure!(
            ret.0.len() == 32,
            "{label}: sendCoin return must be a 32-byte ABI bool, got {} bytes",
            ret.0.len()
        );
        ensure!(
            ret.0.last() == Some(&want),
            "{label}: callee must observe the requested from as msg.sender (want {want}, got {ret:?})"
        );
    }
    Ok(())
}

async fn isolation_call(
    forest: &Client,
    from: EthAddress,
    to: EthAddress,
    block: BlockNumberOrHash,
) -> anyhow::Result<EthBytes> {
    eth_call(forest, from, to, Vec::new(), block).await
}

async fn skip_sender_state_isolation() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let to = EthAddress::from_filecoin_address(deployer().await?)?;
    let from = non_existent(0x03)?;

    let first = isolation_call(&forest, from, to, latest()).await?;
    let second = isolation_call(&forest, from, to, latest()).await?;
    ensure!(
        first == second,
        "repeated skip-sender eth_call results must match"
    );

    let fil = from.to_filecoin_address()?;
    ensure!(
        get_actor(&forest, fil).await?.is_none(),
        "skip-sender eth_call must not persist an actor for {fil}"
    );

    let mut futs = Vec::with_capacity(8);
    for _ in 0..8 {
        futs.push(async move {
            let client = forest_client()?;
            isolation_call(&client, from, to, latest()).await
        });
    }
    let concurrent = futures::future::try_join_all(futs).await?;
    for (i, got) in concurrent.iter().enumerate() {
        ensure!(
            *got == first,
            "concurrent skip-sender eth_call {i} diverged from the first result"
        );
    }
    Ok(())
}

async fn skip_sender_historical_call() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let to = EthAddress::from_filecoin_address(deployer().await?)?;
    let from = non_existent(0x03)?;
    let head = forest
        .call(EthBlockNumber::request(())?)
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    ensure!(
        head.0 > 2,
        "devnet head {} is too low for a head-2 historical eth_call",
        head.0
    );
    let hist = BlockNumberOrHash::from_block_number((head.0 - 2) as i64);
    isolation_call(&forest, from, to, hist)
        .await
        .context("historical skip-sender eth_call at head-2")?;
    Ok(())
}

async fn cross_contract_from_contract() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let (a, b) = linked_contracts().await?;
    assert_abi_u256(
        eth_call(
            &forest,
            b.eth,
            a.eth,
            selector(CALL_B_AND_READ_BACK),
            latest(),
        )
        .await
        .context("callBAndReadBack from contract from")?,
        42,
        "cross-contract callback from contract from",
    )
}

async fn cross_contract_from_missing() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let (a, _b) = linked_contracts().await?;
    assert_abi_u256(
        eth_call(
            &forest,
            non_existent(0x10)?,
            a.eth,
            selector(CALL_B_AND_READ_BACK),
            latest(),
        )
        .await
        .context("callBAndReadBack from missing from")?,
        42,
        "cross-contract callback from missing from",
    )
}

async fn cross_contract_from_eoa() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let (a, _b) = linked_contracts().await?;
    let (_from_f4, from) = new_funded(ROUND_TRIP_FUND_AMT).await?;
    assert_abi_u256(
        eth_call(
            &forest,
            from,
            a.eth,
            selector(CALL_B_AND_READ_BACK),
            latest(),
        )
        .await
        .context("callBAndReadBack from EOA from")?,
        42,
        "cross-contract callback from EOA from",
    )
}

async fn cross_contract_double_callback() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let (a, _b) = linked_contracts().await?;
    assert_abi_u256(
        eth_call(
            &forest,
            non_existent(0x11)?,
            a.eth,
            selector(CALL_B_AND_DOUBLE),
            latest(),
        )
        .await
        .context("callBAndDouble from missing from")?,
        84,
        "cross-contract double callback from missing from",
    )
}

fn assert_abi_u256(ret: EthBytes, expected: u8, label: &str) -> anyhow::Result<()> {
    ensure!(
        ret.0.len() == 32,
        "{label}: expected 32-byte ABI uint256, got {} bytes",
        ret.0.len()
    );
    ensure!(
        ret.0.last() == Some(&expected),
        "{label}: expected {expected}, got {ret:?}"
    );
    Ok(())
}

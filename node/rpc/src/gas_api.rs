// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use num_traits::{FromPrimitive, Zero};
use rand_distr::{Distribution, Normal};

use forest_beacon::Beacon;
use forest_blocks::{tipset_keys_json::TipsetKeysJson, TipsetKeys};
use forest_chain::{BASE_FEE_MAX_CHANGE_DENOM, BLOCK_GAS_TARGET, MINIMUM_BASE_FEE};
use forest_db::Store;
use forest_json::address::json::AddressJson;
use forest_json::message::json::MessageJson;
use forest_message::ChainMessage;
use forest_rpc_api::{
    data_types::{MessageSendSpec, RPCState},
    gas_api::*,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::bigint::BigInt;
use fvm_shared::econ::TokenAmount;
use fvm_shared::message::Message;
use fvm_shared::BLOCK_GAS_LIMIT;

const MIN_GAS_PREMIUM: f64 = 100000.0;

/// Estimate the fee cap
pub(crate) async fn gas_estimate_fee_cap<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<GasEstimateFeeCapParams>,
) -> Result<GasEstimateFeeCapResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (MessageJson(msg), max_queue_blks, TipsetKeysJson(tsk)) = params;

    estimate_fee_cap::<DB, B>(&data, msg, max_queue_blks, tsk).map(|n| TokenAmount::to_string(&n))
}

fn estimate_fee_cap<DB, B>(
    data: &Data<RPCState<DB, B>>,
    msg: Message,
    max_queue_blks: i64,
    _tsk: TipsetKeys,
) -> Result<TokenAmount, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let ts = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .ok_or("can't find heaviest tipset")?;

    let parent_base_fee = ts.blocks()[0].parent_base_fee();
    let increase_factor =
        (1.0 + (BASE_FEE_MAX_CHANGE_DENOM as f64).recip()).powf(max_queue_blks as f64);

    let fee_in_future = parent_base_fee
        * BigInt::from_f64(increase_factor * (1 << 8) as f64)
            .ok_or("failed to convert fee_in_future f64 to bigint")?;
    let mut out = fee_in_future.div_floor(1 << 8);
    out += msg.gas_premium;
    Ok(out)
}

/// Estimate the fee cap
pub(crate) async fn gas_estimate_gas_premium<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<GasEstimateGasPremiumParams>,
) -> Result<GasEstimateGasPremiumResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (nblocksincl, AddressJson(_sender), _gas_limit, TipsetKeysJson(_tsk)) = params;
    estimate_gas_premium::<DB, B>(&data, nblocksincl)
        .await
        .map(|n| TokenAmount::to_string(&n))
}

async fn estimate_gas_premium<DB, B>(
    data: &Data<RPCState<DB, B>>,
    mut nblocksincl: u64,
) -> Result<TokenAmount, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    if nblocksincl == 0 {
        nblocksincl = 1;
    }

    struct GasMeta {
        pub price: TokenAmount,
        pub limit: i64,
    }

    let mut prices: Vec<GasMeta> = Vec::new();
    let mut blocks = 0;

    let mut ts = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .ok_or("cant get heaviest tipset")?;

    for _ in 0..(nblocksincl * 2) {
        if ts.epoch() == 0 {
            break;
        }
        let pts = data
            .state_manager
            .chain_store()
            .tipset_from_keys(ts.parents())?;
        blocks += pts.blocks().len();
        let msgs = forest_chain::messages_for_tipset(data.state_manager.blockstore(), &pts)?;

        prices.append(
            &mut msgs
                .iter()
                .map(|msg| GasMeta {
                    price: msg.message().gas_premium.clone(),
                    limit: msg.message().gas_limit,
                })
                .collect(),
        );
        ts = pts;
    }

    prices.sort_by(|a, b| b.price.cmp(&a.price));
    // TODO: From lotus, account for how full blocks are
    let mut at = BLOCK_GAS_TARGET * blocks as i64 / 2;
    let mut prev = TokenAmount::zero();
    let mut premium = TokenAmount::zero();

    for price in prices {
        at -= price.limit;
        if at > 0 {
            prev = price.price;
            continue;
        }
        if prev == TokenAmount::zero() {
            let ret: TokenAmount = price.price + TokenAmount::from_atto(1);
            return Ok(ret);
        }
        premium = (&price.price + &prev).div_floor(2) + TokenAmount::from_atto(1)
    }

    if premium == TokenAmount::zero() {
        premium = TokenAmount::from_atto(match nblocksincl {
            1 => (MIN_GAS_PREMIUM * 2.0) as u64,
            2 => (MIN_GAS_PREMIUM * 1.5) as u64,
            _ => MIN_GAS_PREMIUM as u64,
        });
    }

    let precision = 32;

    // mean 1, stddev 0.005 => 95% within +-1%
    let noise: f64 = Normal::new(1.0, 0.005)
        .unwrap()
        .sample(&mut rand::thread_rng());

    premium *= BigInt::from_f64(noise * (1i64 << precision) as f64)
        .ok_or("failed to converrt gas premium f64 to bigint")?;
    premium = premium.div_floor(1i64 << precision);

    Ok(premium)
}

/// Estimate the gas limit
pub(crate) async fn gas_estimate_gas_limit<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<GasEstimateGasLimitParams>,
) -> Result<GasEstimateGasLimitResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (MessageJson(msg), TipsetKeysJson(tsk)) = params;
    estimate_gas_limit::<DB, B>(&data, msg, tsk).await
}

async fn estimate_gas_limit<DB, B>(
    data: &Data<RPCState<DB, B>>,
    msg: Message,
    _: TipsetKeys,
) -> Result<i64, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let mut msg = msg;
    msg.gas_limit = BLOCK_GAS_LIMIT;
    msg.gas_fee_cap = TokenAmount::from_atto(MINIMUM_BASE_FEE + 1);
    msg.gas_premium = TokenAmount::from_atto(1);

    let curr_ts = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .ok_or("cant find the current heaviest tipset")?;
    let from_a = data
        .state_manager
        .resolve_to_key_addr(&msg.from, &curr_ts)
        .await?;

    let pending = data.mpool.pending_for(&from_a);
    let prior_messages: Vec<ChainMessage> = pending
        .map(|s| s.into_iter().map(ChainMessage::Signed).collect::<Vec<_>>())
        .unwrap_or_default();

    let ts = data.mpool.cur_tipset.lock().clone();
    let res = data
        .state_manager
        .call_with_gas(&mut ChainMessage::Unsigned(msg), &prior_messages, Some(ts))
        .await?;
    match res.msg_rct {
        Some(rct) => {
            if rct.exit_code.value() != 0 {
                return Ok(-1);
            }
            // TODO: Figure out why we always under estimate the gas calculation so we dont need to add 200000
            // https://github.com/ChainSafe/forest/issues/901
            Ok(rct.gas_used + 200000)
        }
        None => Ok(-1),
    }
}

/// Estimates the gas parameters for a given message
pub(crate) async fn gas_estimate_message_gas<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<GasEstimateMessageGasParams>,
) -> Result<GasEstimateMessageGasResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (MessageJson(msg), spec, TipsetKeysJson(tsk)) = params;
    estimate_message_gas::<DB, B>(&data, msg, spec, tsk)
        .await
        .map(MessageJson::from)
}

pub(crate) async fn estimate_message_gas<DB, B>(
    data: &Data<RPCState<DB, B>>,
    msg: Message,
    _spec: Option<MessageSendSpec>,
    tsk: TipsetKeys,
) -> Result<Message, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let mut msg = msg;
    if msg.gas_limit == 0 {
        let gl = estimate_gas_limit::<DB, B>(data, msg.clone(), tsk.clone()).await?;
        msg.gas_limit = gl;
    }
    if msg.gas_premium.is_zero() {
        let gp = estimate_gas_premium(data, 10).await?;
        msg.gas_premium = gp;
    }
    if msg.gas_fee_cap.is_zero() {
        let gfp = estimate_fee_cap(data, msg.clone(), 20, tsk)?;
        msg.gas_fee_cap = gfp;
    }
    // TODO: Cap Gas Fee https://github.com/ChainSafe/forest/issues/901
    Ok(msg)
}

// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use num_traits::{FromPrimitive, Zero};
use rand_distr::{Distribution, Normal};

use address::json::AddressJson;
use beacon::Beacon;
use blocks::{tipset_keys_json::TipsetKeysJson, TipsetKeys};
use blockstore::BlockStore;
use chain::{BASE_FEE_MAX_CHANGE_DENOM, BLOCK_GAS_TARGET, MINIMUM_BASE_FEE};
use fil_types::{verifier::ProofVerifier, BLOCK_GAS_LIMIT};
use message::{unsigned_message::json::UnsignedMessageJson, UnsignedMessage};
use message::{ChainMessage, Message};
use num_bigint::BigInt;
use rpc_api::{
    data_types::{MessageSendSpec, RPCState},
    gas_api::*,
};

const MIN_GAS_PREMIUM: f64 = 100000.0;

/// Estimate the fee cap
pub(crate) async fn gas_estimate_fee_cap<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<GasEstimateFeeCapParams>,
) -> Result<GasEstimateFeeCapResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (UnsignedMessageJson(msg), max_queue_blks, TipsetKeysJson(tsk)) = params;

    estimate_fee_cap::<DB, B>(&data, msg, max_queue_blks, tsk)
        .await
        .map(|n| BigInt::to_string(&n))
}

async fn estimate_fee_cap<DB, B>(
    data: &Data<RPCState<DB, B>>,
    msg: UnsignedMessage,
    max_queue_blks: i64,
    _tsk: TipsetKeys,
) -> Result<BigInt, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let ts = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or("can't find heaviest tipset")?;

    let parent_base_fee = ts.blocks()[0].parent_base_fee();
    let increase_factor =
        (1.0 + (BASE_FEE_MAX_CHANGE_DENOM as f64).recip()).powf(max_queue_blks as f64);

    let fee_in_future = parent_base_fee
        * BigInt::from_f64(increase_factor * (1 << 8) as f64)
            .ok_or("failed to convert fee_in_future f64 to bigint")?;
    let mut out = fee_in_future / (1 << 8);
    out += msg.gas_premium();
    Ok(out)
}

/// Estimate the fee cap
pub(crate) async fn gas_estimate_gas_premium<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<GasEstimateGasPremiumParams>,
) -> Result<GasEstimateGasPremiumResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (nblocksincl, AddressJson(_sender), _gas_limit, TipsetKeysJson(_tsk)) = params;
    estimate_gas_premium::<DB, B>(&data, nblocksincl)
        .await
        .map(|n| BigInt::to_string(&n))
}

async fn estimate_gas_premium<DB, B>(
    data: &Data<RPCState<DB, B>>,
    mut nblocksincl: u64,
) -> Result<BigInt, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    if nblocksincl == 0 {
        nblocksincl = 1;
    }

    struct GasMeta {
        pub price: BigInt,
        pub limit: i64,
    }

    let mut prices: Vec<GasMeta> = Vec::new();
    let mut blocks = 0;

    let mut ts = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or("cant get heaviest tipset")?;

    for _ in 0..(nblocksincl * 2) {
        if ts.epoch() == 0 {
            break;
        }
        let pts = data
            .state_manager
            .chain_store()
            .tipset_from_keys(ts.parents())
            .await?;
        blocks += pts.blocks().len();
        let msgs = chain::messages_for_tipset(data.state_manager.blockstore(), &pts)?;

        prices.append(
            &mut msgs
                .iter()
                .map(|msg| GasMeta {
                    price: msg.gas_premium().clone(),
                    limit: msg.gas_limit(),
                })
                .collect(),
        );
        ts = pts;
    }

    prices.sort_by(|a, b| b.price.cmp(&a.price));
    // TODO: From lotus, account for how full blocks are
    let mut at = BLOCK_GAS_TARGET * blocks as i64 / 2;
    let mut prev = BigInt::zero();
    let mut premium = BigInt::zero();

    for price in prices {
        at -= price.limit;
        if at > 0 {
            prev = price.price;
            continue;
        }
        if prev == 0.into() {
            let ret: BigInt = price.price + 1;
            return Ok(ret);
        }
        premium = (&price.price + &prev) / 2 + 1
    }

    if premium == 0.into() {
        premium = BigInt::from_f64(match nblocksincl {
            1 => MIN_GAS_PREMIUM * 2.0,
            2 => MIN_GAS_PREMIUM * 1.5,
            _ => MIN_GAS_PREMIUM,
        })
        .ok_or("failed to convert gas premium f64 to bigint")?;
    }

    let precision = 32;

    // mean 1, stddev 0.005 => 95% within +-1%
    let noise: f64 = Normal::new(1.0, 0.005)
        .unwrap()
        .sample(&mut rand::thread_rng());

    premium *= BigInt::from_f64(noise * (1i64 << precision) as f64)
        .ok_or("failed to converrt gas premium f64 to bigint")?;
    premium /= 1i64 << precision;

    Ok(premium)
}

/// Estimate the gas limit
pub(crate) async fn gas_estimate_gas_limit<DB, B, V>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<GasEstimateGasLimitParams>,
) -> Result<GasEstimateGasLimitResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
{
    let (UnsignedMessageJson(msg), TipsetKeysJson(tsk)) = params;
    estimate_gas_limit::<DB, B, V>(&data, msg, tsk).await
}

async fn estimate_gas_limit<DB, B, V>(
    data: &Data<RPCState<DB, B>>,
    msg: UnsignedMessage,
    _: TipsetKeys,
) -> Result<i64, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
{
    let mut msg = msg;
    msg.set_gas_limit(BLOCK_GAS_LIMIT);
    msg.set_gas_fee_cap(MINIMUM_BASE_FEE.clone() + 1);
    msg.set_gas_premium(1.into());

    let curr_ts = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or("cant find the current heaviest tipset")?;
    let from_a = data
        .state_manager
        .resolve_to_key_addr::<V>(msg.from(), &curr_ts)
        .await?;

    let pending = data.mpool.pending_for(&from_a).await;
    let prior_messages: Vec<ChainMessage> = pending
        .map(|s| s.into_iter().map(ChainMessage::Signed).collect::<Vec<_>>())
        .unwrap_or_default();

    let res = data
        .state_manager
        .call_with_gas::<V>(
            &mut ChainMessage::Unsigned(msg),
            &prior_messages,
            Some(data.mpool.cur_tipset.as_ref().read().await.clone()),
        )
        .await?;
    match res.msg_rct {
        Some(rct) => {
            if rct.exit_code as u64 != 0 {
                return Ok(-1);
            }
            // TODO: Figure out why we always under estimate the gas calculation so we dont need to add 200000
            // https://github.com/ChainSafe/forest/issues/901
            Ok(rct.gas_used + 200000)
        }
        None => Ok(-1),
    }
}

/// Estimates the gas paramaters for a given message
pub(crate) async fn gas_estimate_message_gas<DB, B, V>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<GasEstimateMessageGasParams>,
) -> Result<GasEstimateMessageGasResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
{
    let (UnsignedMessageJson(msg), spec, TipsetKeysJson(tsk)) = params;
    estimate_message_gas::<DB, B, V>(&data, msg, spec, tsk)
        .await
        .map(UnsignedMessageJson::from)
}

pub(crate) async fn estimate_message_gas<DB, B, V>(
    data: &Data<RPCState<DB, B>>,
    msg: UnsignedMessage,
    _spec: Option<MessageSendSpec>,
    tsk: TipsetKeys,
) -> Result<UnsignedMessage, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
{
    let mut msg = msg;
    if msg.gas_limit() == 0 {
        let gl = estimate_gas_limit::<DB, B, V>(&data, msg.clone(), tsk.clone()).await?;
        msg.gas_limit = gl;
    }
    if msg.gas_premium().is_zero() {
        let gp = estimate_gas_premium(&data, 10).await?;
        msg.gas_premium = gp;
    }
    if msg.gas_fee_cap().is_zero() {
        let gfp = estimate_fee_cap(&data, msg.clone(), 20, tsk).await?;
        msg.gas_fee_cap = gfp;
    }
    // TODO: Cap Gas Fee https://github.com/ChainSafe/forest/issues/901
    Ok(msg)
}

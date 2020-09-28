// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;
use address::Address;
use blocks::TipsetKeys;
use blockstore::BlockStore;
use chain::{BASE_FEE_MAX_CHANGE_DENOM, BLOCK_GAS_TARGET, MINIMUM_BASE_FEE};
use fil_types::BLOCK_GAS_LIMIT;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::unsigned_message::json::UnsignedMessageJson;
use message::{ChainMessage, Message};
use num_bigint::BigInt;
use num_traits::{FromPrimitive, Zero};
use rand_distr::{Distribution, Normal};
use wallet::KeyStore;
const MIN_GAS_PREMIUM: f64 = 100000.0;
const MAX_SPEND_ON_FEE_DENOM: i64 = 100;

/// Estimate the fee cap
pub(crate) async fn gas_estimate_fee_cap<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(UnsignedMessageJson, i64, TipsetKeys)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (UnsignedMessageJson(msg), max_queue_blks, _tsk) = params;

    let ts = chain::get_heaviest_tipset(data.state_manager.blockstore())?
        .ok_or("can't find heaviest tipset")?;

    let act = data
        .state_manager
        .get_actor(msg.from(), ts.parent_state())?
        .ok_or("could not load actor")?;

    let parent_base_fee = ts.blocks()[0].parent_base_fee();
    let increase_factor =
        (1.0 + (BASE_FEE_MAX_CHANGE_DENOM as f64).recip()).powf(max_queue_blks as f64);

    let fee_in_future = parent_base_fee
        * BigInt::from_f64(increase_factor * (1 << 8) as f64)
            .ok_or("failed to convert fee_in_future f64 to bigint")?;
    let fee_in_future = fee_in_future / (1 << 8);

    let gas_limit_big: BigInt = msg.gas_limit().into();
    let max_accepted = act.balance / MAX_SPEND_ON_FEE_DENOM;
    let expected_fee = &fee_in_future * &gas_limit_big;

    let out = if expected_fee > max_accepted {
        max_accepted / gas_limit_big
    } else {
        fee_in_future
    };
    Ok(out.to_string())
}

/// Estimate the fee cap
pub(crate) async fn gas_estimate_gas_premium<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(u64, Address, i64, TipsetKeys)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (mut nblocksincl, _sender, _gas_limit, _) = params;

    if nblocksincl == 0 {
        nblocksincl = 1;
    }

    struct GasMeta {
        pub price: BigInt,
        pub limit: i64,
    }

    let mut prices: Vec<GasMeta> = Vec::new();
    let mut blocks = 0;

    let mut ts = chain::get_heaviest_tipset(data.state_manager.blockstore())?
        .ok_or("cant get heaviest tipset")?;

    for _ in 0..(nblocksincl * 2) {
        if ts.parents().cids().is_empty() {
            break;
        }
        let pts = chain::tipset_from_keys(data.state_manager.blockstore(), ts.parents())?;
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
            return Ok(ret.to_string());
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
    premium *= BigInt::from_f64(noise * (1 << precision) as f64)
        .ok_or("failed to converrt gas premium f64 to bigint")?;
    premium /= 1 << precision;

    Ok(premium.to_string())
}

/// Estimate the gas limit
pub(crate) async fn gas_estimate_gas_limit<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(UnsignedMessageJson, TipsetKeys)>,
) -> Result<i64, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (UnsignedMessageJson(mut msg), _) = params;
    msg.set_gas_limit(BLOCK_GAS_LIMIT);
    msg.set_gas_fee_cap(MINIMUM_BASE_FEE.clone() + 1);
    msg.set_gas_premium(1.into());

    let curr_ts = chain::get_heaviest_tipset(data.state_manager.blockstore())?
        .ok_or("cant find the current heaviest tipset")?;
    let from_a = data
        .state_manager
        .resolve_to_key_addr(msg.from(), &curr_ts)
        .await?;

    let pending = data.mpool.pending_for(&from_a).await;
    let prior_messages: Vec<ChainMessage> = pending
        .map(|s| s.into_iter().map(ChainMessage::Signed).collect::<Vec<_>>())
        .unwrap_or_default();
    let res = data
        .state_manager
        .call_with_gas(
            &mut msg,
            &prior_messages,
            Some(data.mpool.cur_tipset.as_ref().read().await.clone()),
        )
        .await?;
    match res.msg_rct {
        Some(rct) => {
            if rct.exit_code as u64 != 0 {
                return Ok(-1);
            }
            Ok(rct.gas_used)
        }
        None => Ok(-1),
    }
}

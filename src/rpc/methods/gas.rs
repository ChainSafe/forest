// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::blocks::Tipset;
use crate::chain::{BASE_FEE_MAX_CHANGE_DENOM, BLOCK_GAS_TARGET};
use crate::interpreter::VMTrace;
use crate::message::{ChainMessage, Message as MessageTrait, SignedMessage};
use crate::rpc::{error::ServerError, types::*, ApiPaths, Ctx, Permission, RpcMethod};
use crate::shim::executor::ApplyRet;
use crate::shim::{
    address::{Address, Protocol},
    crypto::{Signature, SignatureType, SECP_SIG_LEN},
    econ::{TokenAmount, BLOCK_GAS_LIMIT},
    message::Message,
};
use anyhow::{Context, Result};
use fvm_ipld_blockstore::Blockstore;
use num::BigInt;
use num_traits::{FromPrimitive, Zero};
use rand_distr::{Distribution, Normal};

use super::state::InvocResult;

const MIN_GAS_PREMIUM: f64 = 100000.0;

/// Estimate the fee cap
pub enum GasEstimateFeeCap {}
impl RpcMethod<3> for GasEstimateFeeCap {
    const NAME: &'static str = "Filecoin.GasEstimateFeeCap";
    const PARAM_NAMES: [&'static str; 3] = ["message", "maxQueueBlocks", "tipsetKey"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Message, i64, ApiTipsetKey);
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msg, max_queue_blks, tsk): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        estimate_fee_cap(&ctx, msg, max_queue_blks, tsk).map(|n| TokenAmount::to_string(&n))
    }
}

fn estimate_fee_cap<DB: Blockstore>(
    data: &Ctx<DB>,
    msg: Message,
    max_queue_blks: i64,
    _: ApiTipsetKey,
) -> Result<TokenAmount, ServerError> {
    let ts = data.chain_store().heaviest_tipset();

    let parent_base_fee = &ts.block_headers().first().parent_base_fee;
    let increase_factor =
        (1.0 + (BASE_FEE_MAX_CHANGE_DENOM as f64).recip()).powf(max_queue_blks as f64);

    let fee_in_future = parent_base_fee
        * BigInt::from_f64(increase_factor * (1 << 8) as f64)
            .context("failed to convert fee_in_future f64 to bigint")?;
    let mut out: crate::shim::econ::TokenAmount = fee_in_future.div_floor(1 << 8);
    out += msg.gas_premium();
    Ok(out)
}

/// Estimate the fee cap
pub enum GasEstimateGasPremium {}
impl RpcMethod<4> for GasEstimateGasPremium {
    const NAME: &'static str = "Filecoin.GasEstimateGasPremium";
    const PARAM_NAMES: [&'static str; 4] = [
        "numberOfBlocksToInclude",
        "senderAddress",
        "gasLimit",
        "tipsetKey",
    ];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (u64, Address, i64, ApiTipsetKey);
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (nblocksincl, _sender, _gas_limit, _tsk): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        estimate_gas_premium(&ctx, nblocksincl)
            .await
            .map(|n| TokenAmount::to_string(&n))
    }
}

pub async fn estimate_gas_premium<DB: Blockstore>(
    data: &Ctx<DB>,
    mut nblocksincl: u64,
) -> Result<TokenAmount, ServerError> {
    if nblocksincl == 0 {
        nblocksincl = 1;
    }

    struct GasMeta {
        pub price: TokenAmount,
        pub limit: u64,
    }

    let mut prices: Vec<GasMeta> = Vec::new();
    let mut blocks = 0;

    let mut ts = data.chain_store().heaviest_tipset();

    for _ in 0..(nblocksincl * 2) {
        if ts.epoch() == 0 {
            break;
        }
        let pts = data.chain_index().load_required_tipset(ts.parents())?;
        blocks += pts.block_headers().len();
        let msgs = crate::chain::messages_for_tipset(data.store_owned(), &pts)?;

        prices.append(
            &mut msgs
                .iter()
                .map(|msg| GasMeta {
                    price: msg.message().gas_premium(),
                    limit: msg.message().gas_limit(),
                })
                .collect(),
        );
        ts = pts;
    }

    prices.sort_by(|a, b| b.price.cmp(&a.price));
    let mut at = BLOCK_GAS_TARGET * blocks as u64 / 2;
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
        .context("failed to convert gas premium f64 to bigint")?;
    premium = premium.div_floor(1i64 << precision);

    Ok(premium)
}

pub enum GasEstimateGasLimit {}
impl RpcMethod<2> for GasEstimateGasLimit {
    const NAME: &'static str = "Filecoin.GasEstimateGasLimit";
    const PARAM_NAMES: [&'static str; 2] = ["message", "tipsetKey"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Message, ApiTipsetKey);
    type Ok = i64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msg, tsk): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::estimate_gas_limit(&ctx, msg, &tsk).await?)
    }
}

impl GasEstimateGasLimit {
    pub async fn estimate_call_with_gas<DB>(
        data: &Ctx<DB>,
        mut msg: Message,
        ApiTipsetKey(tsk): &ApiTipsetKey,
        trace_config: VMTrace,
    ) -> anyhow::Result<(InvocResult, ApplyRet, Vec<ChainMessage>, Arc<Tipset>)>
    where
        DB: Blockstore + Send + Sync + 'static,
    {
        msg.set_gas_limit(BLOCK_GAS_LIMIT);
        msg.set_gas_fee_cap(TokenAmount::from_atto(0));
        msg.set_gas_premium(TokenAmount::from_atto(0));

        let curr_ts = data.chain_store().load_required_tipset_or_heaviest(tsk)?;
        let from_a = data
            .state_manager
            .resolve_to_key_addr(&msg.from, &curr_ts)
            .await?;

        let pending = data.mpool.pending_for(&from_a);
        let prior_messages: Vec<ChainMessage> = pending
            .map(|s| s.into_iter().map(ChainMessage::Signed).collect::<Vec<_>>())
            .unwrap_or_default();

        let ts = data.mpool.cur_tipset.lock().clone();
        // Pretend that the message is signed. This has an influence on the gas
        // cost. We obviously can't generate a valid signature. Instead, we just
        // fill the signature with zeros. The validity is not checked.
        let mut chain_msg = match from_a.protocol() {
            Protocol::Secp256k1 => ChainMessage::Signed(SignedMessage::new_unchecked(
                msg,
                Signature::new_secp256k1(vec![0; SECP_SIG_LEN]),
            )),
            Protocol::Delegated => ChainMessage::Signed(SignedMessage::new_unchecked(
                msg,
                // In Lotus, delegated signatures have the same length as SECP256k1.
                // This may or may not change in the future.
                Signature::new(SignatureType::Delegated, vec![0; SECP_SIG_LEN]),
            )),
            _ => ChainMessage::Unsigned(msg),
        };

        let (invoc_res, apply_ret) = data
            .state_manager
            .call_with_gas(
                &mut chain_msg,
                &prior_messages,
                Some(ts.clone()),
                trace_config,
            )
            .await?;
        Ok((invoc_res, apply_ret, prior_messages, ts))
    }

    pub async fn estimate_gas_limit<DB>(
        data: &Ctx<DB>,
        msg: Message,
        tsk: &ApiTipsetKey,
    ) -> anyhow::Result<i64>
    where
        DB: Blockstore + Send + Sync + 'static,
    {
        let (res, ..) = Self::estimate_call_with_gas(data, msg, tsk, VMTrace::NotTraced).await?;
        match res.msg_rct {
            Some(rct) => {
                if rct.exit_code().value() != 0 {
                    return Ok(-1);
                }
                Ok(rct.gas_used() as i64)
            }
            None => Ok(-1),
        }
    }
}

/// Estimates the gas parameters for a given message
pub enum GasEstimateMessageGas {}
impl RpcMethod<3> for GasEstimateMessageGas {
    const NAME: &'static str = "Filecoin.GasEstimateMessageGas";
    const PARAM_NAMES: [&'static str; 3] = ["message", "messageSendSpec", "tipsetKey"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Message, Option<MessageSendSpec>, ApiTipsetKey);
    type Ok = Message;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msg, spec, tsk): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        estimate_message_gas(&ctx, msg, spec, tsk).await
    }
}

pub async fn estimate_message_gas<DB>(
    data: &Ctx<DB>,
    mut msg: Message,
    _spec: Option<MessageSendSpec>,
    tsk: ApiTipsetKey,
) -> Result<Message, ServerError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    if msg.gas_limit == 0 {
        let gl = GasEstimateGasLimit::estimate_gas_limit(data, msg.clone(), &tsk).await?;
        let gl = gl as f64 * data.mpool.config.gas_limit_overestimation;
        msg.set_gas_limit((gl as u64).min(BLOCK_GAS_LIMIT));
    }
    if msg.gas_premium.is_zero() {
        let gp = estimate_gas_premium(data, 10).await?;
        msg.set_gas_premium(gp);
    }
    if msg.gas_fee_cap.is_zero() {
        let gfp = estimate_fee_cap(data, msg.clone(), 20, tsk)?;
        msg.set_gas_fee_cap(gfp);
    }
    Ok(msg)
}

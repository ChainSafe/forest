// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::state::InvocResult;
use crate::blocks::Tipset;
use crate::chain::{BASE_FEE_MAX_CHANGE_DENOM, BLOCK_GAS_TARGET};
use crate::interpreter::VMTrace;
use crate::message::{ChainMessage, Message as MessageTrait, SignedMessage};
use crate::rpc::chain::FlattenedApiMessage;
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod, error::ServerError, types::*};
use crate::shim::executor::ApplyRet;
use crate::shim::{
    address::{Address, Protocol},
    crypto::{SECP_SIG_LEN, Signature, SignatureType},
    econ::{BLOCK_GAS_LIMIT, TokenAmount},
    message::Message,
};
use crate::state_manager::{StateLookupPolicy, VMFlush};
use anyhow::{Context, Result};
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools as _;
use num::BigInt;
use num_traits::{FromPrimitive, Zero};
use rand_distr::{Distribution, Normal};
use std::ops::Add;

const MIN_GAS_PREMIUM: f64 = 100000.0;

/// Estimate the fee cap
pub enum GasEstimateFeeCap {}
impl RpcMethod<3> for GasEstimateFeeCap {
    const NAME: &'static str = "Filecoin.GasEstimateFeeCap";
    const PARAM_NAMES: [&'static str; 3] = ["message", "maxQueueBlocks", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the estimated fee cap for the given parameters.");

    type Params = (Message, i64, ApiTipsetKey);
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msg, max_queue_blks, tsk): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        estimate_fee_cap(&ctx, &msg, max_queue_blks, &tsk).map(|n| TokenAmount::to_string(&n))
    }
}

fn estimate_fee_cap<DB: Blockstore>(
    data: &Ctx<DB>,
    msg: &Message,
    max_queue_blks: i64,
    ApiTipsetKey(ts_key): &ApiTipsetKey,
) -> Result<TokenAmount, ServerError> {
    let ts = data
        .chain_store()
        .load_required_tipset_or_heaviest(ts_key)?;

    let parent_base_fee = &ts.block_headers().first().parent_base_fee;
    let increase_factor =
        (1.0 + (BASE_FEE_MAX_CHANGE_DENOM as f64).recip()).powf(max_queue_blks as f64);

    let fee_in_future = parent_base_fee
        * BigInt::from_f64(increase_factor * (1 << 8) as f64)
            .context("failed to convert fee_in_future f64 to bigint")?;
    let out = fee_in_future.div_floor(1 << 8).add(msg.gas_premium());
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
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the estimated gas premium for the given parameters.");

    type Params = (u64, Address, i64, ApiTipsetKey);
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (nblocksincl, _sender, _gas_limit, tsk): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        estimate_gas_premium(&ctx, nblocksincl, &tsk)
            .await
            .map(|n| TokenAmount::to_string(&n))
    }
}

#[derive(Clone)]
struct GasMeta {
    pub price: TokenAmount,
    pub limit: u64,
}

pub async fn estimate_gas_premium<DB: Blockstore>(
    data: &Ctx<DB>,
    mut nblocksincl: u64,
    ApiTipsetKey(ts_key): &ApiTipsetKey,
) -> Result<TokenAmount, ServerError> {
    if nblocksincl == 0 {
        nblocksincl = 1;
    }

    let mut prices: Vec<GasMeta> = Vec::new();
    let mut blocks = 0;

    let mut ts = data
        .chain_store()
        .load_required_tipset_or_heaviest(ts_key)?;

    for _ in 0..(nblocksincl * 2) {
        if ts.epoch() == 0 {
            break;
        }
        let pts = data.chain_index().load_required_tipset(ts.parents())?;
        blocks += pts.block_headers().len();
        let msgs =
            crate::chain::messages_for_tipset_with_cache(data.store(), &pts, &data.msgs_in_tipset)?;

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

    let mut premium = compute_gas_premium(prices, blocks as u64);

    if premium < TokenAmount::from_atto(MIN_GAS_PREMIUM as u64) {
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
        .sample(&mut crate::utils::rand::forest_rng());

    premium *= BigInt::from_f64((noise * (1i64 << precision) as f64) + 1f64)
        .context("failed to convert gas premium f64 to bigint")?;
    premium = premium.div_floor(1i64 << precision);

    Ok(premium)
}

// logic taken from here <https://github.com/filecoin-project/lotus/blob/v1.34.3/node/impl/gasutils/gasutils.go#L302>
fn compute_gas_premium(mut prices: Vec<GasMeta>, blocks: u64) -> TokenAmount {
    prices.sort_by(|a, b| b.price.cmp(&a.price));

    let mut at = BLOCK_GAS_TARGET * blocks / 2;
    at += BLOCK_GAS_TARGET * blocks / (2 * 20);

    let mut prev1 = TokenAmount::zero();
    let mut prev2 = TokenAmount::zero();

    for p in prices {
        prev2 = prev1.clone();
        prev1 = p.price.clone();

        if p.limit > at {
            // We've crossed the threshold
            break;
        }
        at -= p.limit;
    }

    if prev2.is_zero() {
        prev1
    } else {
        (&prev1 + &prev2).div_floor(2)
    }
}

pub enum GasEstimateGasLimit {}
impl RpcMethod<2> for GasEstimateGasLimit {
    const NAME: &'static str = "Filecoin.GasEstimateGasLimit";
    const PARAM_NAMES: [&'static str; 2] = ["message", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the estimated gas limit for the given parameters.");

    type Params = (Message, ApiTipsetKey);
    type Ok = i64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msg, tsk): Self::Params,
        _: &http::Extensions,
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
    ) -> anyhow::Result<(InvocResult, ApplyRet, Vec<ChainMessage>, Tipset)>
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
            .map(|s| s.into_iter().map(ChainMessage::Signed).collect_vec())
            .unwrap_or_default();

        let ts = data.mpool.current_tipset();
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

        let (invoc_res, apply_ret, _, _) = data
            .state_manager
            .call_with_gas(
                &mut chain_msg,
                &prior_messages,
                Some(ts.clone()),
                trace_config,
                StateLookupPolicy::Enabled,
                VMFlush::Skip,
            )
            .await?;
        Ok((invoc_res, apply_ret, prior_messages, ts))
    }

    pub async fn estimate_gas_limit<DB>(
        data: &Ctx<DB>,
        msg: Message,
        tsk: &ApiTipsetKey,
    ) -> Result<i64>
    where
        DB: Blockstore + Send + Sync + 'static,
    {
        let (res, ..) = Self::estimate_call_with_gas(data, msg, tsk, VMTrace::NotTraced)
            .await
            .map_err(|e| anyhow::anyhow!("gas estimation failed: {e}"))?;
        match res.msg_rct {
            Some(rct) => {
                anyhow::ensure!(
                    rct.exit_code().is_success(),
                    "message execution failed: exit code: {}, reason: {}",
                    rct.exit_code().value(),
                    res.error.unwrap_or_default()
                );
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
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the estimated gas for the given parameters.");

    type Params = (Message, Option<MessageSendSpec>, ApiTipsetKey);
    type Ok = FlattenedApiMessage;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msg, spec, tsk): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let message = estimate_message_gas(&ctx, msg, spec, tsk).await?;
        let cid = message.cid();
        Ok(FlattenedApiMessage { message, cid })
    }
}

pub async fn estimate_message_gas<DB>(
    data: &Ctx<DB>,
    mut msg: Message,
    msg_spec: Option<MessageSendSpec>,
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
        let gp = estimate_gas_premium(data, 10, &tsk).await?;
        msg.set_gas_premium(gp);
    }
    if msg.gas_fee_cap.is_zero() {
        let gfp = estimate_fee_cap(data, &msg, 20, &tsk)?;
        msg.set_gas_fee_cap(gfp);
    }

    cap_gas_fee(&data.chain_config().default_max_fee, &mut msg, msg_spec)?;

    Ok(msg)
}

/// Caps the gas fee to ensure it doesn't exceed the maximum allowed fee.
/// Returns an error if the msg `gas_limit` is zero
fn cap_gas_fee(
    default_max_fee: &TokenAmount,
    msg: &mut Message,
    msg_spec: Option<MessageSendSpec>,
) -> Result<()> {
    let gas_limit = msg.gas_limit();
    anyhow::ensure!(gas_limit > 0, "gas limit must be positive for fee capping");

    let (maximize_fee_cap, max_fee) = match &msg_spec {
        Some(spec) => (
            spec.maximize_fee_cap,
            if spec.max_fee.is_zero() {
                default_max_fee
            } else {
                &spec.max_fee
            },
        ),
        None => (false, default_max_fee),
    };

    let total_fee = msg.gas_fee_cap() * gas_limit;
    if !max_fee.is_zero() && (maximize_fee_cap || total_fee > *max_fee) {
        msg.set_gas_fee_cap(max_fee.div_floor(gas_limit));
    }

    // cap premium at FeeCap
    msg.set_gas_premium(msg.gas_fee_cap().min(msg.gas_premium()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shim::econ::TokenAmount;
    use crate::utils;

    #[test]
    fn test_compute_gas_premium_single_entry() {
        // Test with single entry at full block gas target
        let prices = vec![GasMeta {
            price: TokenAmount::from_atto(5),
            limit: BLOCK_GAS_TARGET,
        }];
        let result = compute_gas_premium(prices, 1);
        assert_eq!(result, TokenAmount::from_atto(5));
    }

    #[test]
    fn test_compute_gas_premium_two_entries() {
        // Test with two entries, each at full block gas target
        // Function will sort by price descending: [10, 5]
        // With 1 block: at = BLOCK_GAS_TARGET/2 + BLOCK_GAS_TARGET/40 = 2.625B gas
        // First entry (10): limit = 5B > 2.625B, so we stop immediately and return first price
        let prices = vec![
            GasMeta {
                price: TokenAmount::from_atto(5),
                limit: BLOCK_GAS_TARGET,
            },
            GasMeta {
                price: TokenAmount::from_atto(10),
                limit: BLOCK_GAS_TARGET,
            },
        ];
        let result = compute_gas_premium(prices, 1);
        assert_eq!(result, TokenAmount::from_atto(10));
    }

    #[test]
    fn test_compute_gas_premium_half_block_entries_single_block() {
        // Test with entries at half-block gas target, single block
        // Function will sort by price descending: [20, 10]
        let prices = vec![
            GasMeta {
                price: TokenAmount::from_atto(10),
                limit: BLOCK_GAS_TARGET / 2,
            },
            GasMeta {
                price: TokenAmount::from_atto(20),
                limit: BLOCK_GAS_TARGET / 2,
            },
        ];
        let result = compute_gas_premium(prices, 1);
        assert_eq!(result, TokenAmount::from_atto(15));
    }

    #[test]
    fn test_compute_gas_premium_three_entries_two_blocks() {
        // Test with three entries at a half-block gas target, two blocks
        // Function will sort by price descending: [30, 20, 10]
        // With 2 blocks: at = BLOCK_GAS_TARGET + BLOCK_GAS_TARGET/20 = 5.25B gas
        // First entry (30): at = 5.25B - 2.5B = 2.75B remaining
        // Second entry (20): at = 2.75B - 2.5B = 0.25B remaining
        // Third entry (10): limit = 2.5B > 0.25B, so we stop and average second and third
        let prices = vec![
            GasMeta {
                price: TokenAmount::from_atto(10),
                limit: BLOCK_GAS_TARGET / 2,
            },
            GasMeta {
                price: TokenAmount::from_atto(20),
                limit: BLOCK_GAS_TARGET / 2,
            },
            GasMeta {
                price: TokenAmount::from_atto(30),
                limit: BLOCK_GAS_TARGET / 2,
            },
        ];
        let result = compute_gas_premium(prices, 2);
        let expected = (TokenAmount::from_atto(20) + TokenAmount::from_atto(10)).div_floor(2);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_compute_gas_premium_empty_list() {
        // Test with empty price list
        let prices = vec![];
        let result = compute_gas_premium(prices, 1);
        assert_eq!(result, TokenAmount::zero());
    }

    #[test]
    fn test_compute_gas_premium_large_gas_limits() {
        // Test with entries that have gas limits larger than the threshold
        // Function will sort by price descending: [100, 50]
        let prices = vec![
            GasMeta {
                price: TokenAmount::from_atto(100),
                limit: BLOCK_GAS_TARGET * 2, // Exceeds threshold immediately
            },
            GasMeta {
                price: TokenAmount::from_atto(50),
                limit: BLOCK_GAS_TARGET / 4,
            },
        ];
        let result = compute_gas_premium(prices, 1);
        assert_eq!(result, TokenAmount::from_atto(100));
    }

    #[test]
    fn test_compute_gas_premium_unsorted_input() {
        // Test that function correctly handles unsorted input (sorting is done internally)
        // Input order: [10, 30, 20] -> After internal sorting: [30, 20, 10]
        let prices = vec![
            GasMeta {
                price: TokenAmount::from_atto(10),
                limit: BLOCK_GAS_TARGET / 4,
            },
            GasMeta {
                price: TokenAmount::from_atto(30),
                limit: BLOCK_GAS_TARGET / 4,
            },
            GasMeta {
                price: TokenAmount::from_atto(20),
                limit: BLOCK_GAS_TARGET / 4,
            },
        ];

        let result = compute_gas_premium(prices, 1);
        let expected = (TokenAmount::from_atto(20) + TokenAmount::from_atto(10)).div_floor(2);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_compute_gas_premium_multiple_blocks() {
        // Test with multiple blocks affecting the threshold calculation
        // Function will sort by price descending: [40, 30, 20, 10]
        let prices = vec![
            GasMeta {
                price: TokenAmount::from_atto(40),
                limit: BLOCK_GAS_TARGET / 4,
            },
            GasMeta {
                price: TokenAmount::from_atto(30),
                limit: BLOCK_GAS_TARGET / 4,
            },
            GasMeta {
                price: TokenAmount::from_atto(20),
                limit: BLOCK_GAS_TARGET / 4,
            },
            GasMeta {
                price: TokenAmount::from_atto(10),
                limit: BLOCK_GAS_TARGET / 4,
            },
        ];

        // With 3 blocks, threshold is higher, so we should get a different result
        let result_1_block = compute_gas_premium(prices.clone(), 1);
        let result_3_blocks = compute_gas_premium(prices, 3);

        // With more blocks, the threshold is higher, so we should pick a lower price
        assert!(result_3_blocks <= result_1_block);
    }

    // Helper function to create a test message with gas parameters
    fn create_test_message(gas_limit: u64, gas_fee_cap: u64, gas_premium: u64) -> Message {
        Message {
            from: Address::new_id(1000),
            to: Address::new_id(1001),
            gas_limit,
            gas_fee_cap: TokenAmount::from_atto(gas_fee_cap),
            gas_premium: TokenAmount::from_atto(gas_premium),
            ..Default::default()
        }
    }

    #[test]
    fn test_cap_gas_fee_within_limit() {
        // Normal case: total fee is within default max fee
        let default_max_fee = TokenAmount::from_atto(1_000_000);
        let mut msg = create_test_message(1000, 500, 100);

        cap_gas_fee(&default_max_fee, &mut msg, None).unwrap();

        assert_eq!(msg.gas_fee_cap(), TokenAmount::from_atto(500));
        assert_eq!(msg.gas_premium(), TokenAmount::from_atto(100));
    }

    #[test]
    fn test_cap_gas_fee_exceeds_limit() {
        // Fee exceeds max: should cap gas_fee_cap
        let default_max_fee = TokenAmount::from_atto(500_000);
        let mut msg = create_test_message(1000, 1000, 200);

        cap_gas_fee(&default_max_fee, &mut msg, None).unwrap();

        assert_eq!(msg.gas_fee_cap(), TokenAmount::from_atto(500));
        assert_eq!(msg.gas_premium(), TokenAmount::from_atto(200));
    }

    #[test]
    fn test_cap_gas_fee_premium_exceeds_fee_cap() {
        // Premium exceeds fee cap after capping: premium should be capped too
        let default_max_fee = TokenAmount::from_atto(300_000);
        let mut msg = create_test_message(1000, 1000, 800);

        cap_gas_fee(&default_max_fee, &mut msg, None).unwrap();

        assert_eq!(msg.gas_fee_cap(), TokenAmount::from_atto(300));
        assert_eq!(msg.gas_premium(), TokenAmount::from_atto(300));
    }

    #[test]
    fn test_cap_gas_fee_maximize_flag() {
        // maximize_fee_cap flag: should set gas_fee_cap to max even if within limit
        let default_max_fee = TokenAmount::from_atto(1_000_000);
        let mut msg = create_test_message(1000, 500, 100);

        let spec = MessageSendSpec {
            max_fee: TokenAmount::zero(),
            msg_uuid: utils::rand::new_uuid_v4(),
            maximize_fee_cap: true,
        };

        cap_gas_fee(&default_max_fee, &mut msg, Some(spec)).unwrap();

        assert_eq!(msg.gas_fee_cap(), TokenAmount::from_atto(1000));
        assert_eq!(msg.gas_premium(), TokenAmount::from_atto(100));
    }

    #[test]
    fn test_cap_gas_fee_zero_gas_limit() {
        // Edge case: zero gas_limit should return an error
        let default_max_fee = TokenAmount::from_atto(1_000_000);
        let mut msg = create_test_message(0, 1000, 200);

        let result = cap_gas_fee(&default_max_fee, &mut msg, None);

        assert!(result.is_err());
    }
}

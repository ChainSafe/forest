use crate::RpcState;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
// use std::primitive::
use blocks::TipsetKeys;
use wallet::KeyStore;
use blockstore::BlockStore;
use message::unsigned_message::json::UnsignedMessageJson;
use address::Address;
use chain::{BASE_FEE_MAX_CHANGE_DENOM, BLOCK_GAS_TARGET};
use num_bigint::{BigInt, ToBigInt};
use message::Message;
use num_traits::{Zero, FromPrimitive};
use rand_distr::{Normal, Distribution};
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
    let (UnsignedMessageJson(msg), max_queue_blks, tsk) = params;

    let ts = chain::get_heaviest_tipset(data.store.as_ref())?.ok_or("can't find heaviest tipset")?;


    let parent_base_fee = ts.blocks()[0].parent_base_fee();
    let increase_factor = (1.0 + (BASE_FEE_MAX_CHANGE_DENOM as f64).recip()).powf(20.0) ;

    let fee_in_future = parent_base_fee * BigInt::from_f64(increase_factor * (1<<8)).ok_or("failed to convert fee_in_future f64 to bigint")?;
    let fee_in_future = fee_in_future / (1<<8);

    let gas_limit_big: BigInt= msg.gas_limit().into();
    // let max_accepted =



    // let mut out = parent_base_fee * (increase_factor * (1<<8) as f64);
    // out /= 1<<8;
    //
    // Ok(out.to_str())

    todo!()
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
    let (mut nblocksincl, sender, gas_limit, _) = params;

    if nblocksincl == 0 {
        nblocksincl = 1;
    }

    struct GasMeta {
        pub price: BigInt,
        pub limit: i64,
    }

    let mut prices: Vec<GasMeta> = Vec::new();
    let mut blocks = 0;

    let mut ts = chain::get_heaviest_tipset(data.store.as_ref())?.ok_or("cant get heaviest tipset")?;

    for i in 0..(nblocksincl*2) {
        if ts.parents().cids().is_empty() {
            break;
        }
        let pts = chain::tipset_from_keys(data.store.as_ref(), ts.parents())?;
        blocks += pts.blocks().len();
        let  msgs = chain::messages_for_tipset(data.store.as_ref(), &pts)?;

        prices.append(&mut msgs.iter().map(|msg| GasMeta{
            price: msg.gas_premium().clone(),
            limit: msg.gas_limit(),
        }).collect());
        ts = pts;
    }

    prices.sort_by(|a, b| b.price.cmp(&a.price));
    // TODO: From lotus, account for how full blocks are
    let mut at = BLOCK_GAS_TARGET * blocks as i64 / 2;
    let  mut prev = BigInt::zero();
    let  mut premium = BigInt::zero();

    for price in prices {
        at -= price.limit;
        if at > 0 {
            prev = price.price;
            continue;
        }
        if &prev == &0.into() {
            let ret: BigInt = price.price + 1;
            return Ok(ret.to_string());
        }
        premium = ((&price.price + &prev) / 2 + 1)

    }

    if premium == 0.into() {
        premium = BigInt::from_f64( match nblocksincl {
            1 => MIN_GAS_PREMIUM * 2.0,
            2 => MIN_GAS_PREMIUM * 1.5,
            _ => MIN_GAS_PREMIUM,
        }).ok_or("failed to convert gas premium f64 to bigint")?;
    }

    let precision = 32;

    // mean 1, stddev 0.005 => 95% within +-1%
    let noise: f64 = Normal::new(1.0, 0.005).unwrap().sample(&mut rand::thread_rng());
    premium *= BigInt::from_f64(noise * (1<<precision) as f64).ok_or("failed to converrt gas premium f64 to bigint")?;
    premium /= (1<<precision);

    Ok(premium.to_string())
}



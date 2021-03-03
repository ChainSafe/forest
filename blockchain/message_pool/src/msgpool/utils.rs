// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::Error;
use chain::MINIMUM_BASE_FEE;
use cid::Cid;
use crypto::Signature;
use encoding::Cbor;
use lru::LruCache;
use message::{Message, SignedMessage, UnsignedMessage};
use num_bigint::{BigInt, Integer};
use num_rational::BigRational;
use num_traits::ToPrimitive;

pub(crate) fn get_base_fee_lower_bound(base_fee: &BigInt, factor: i64) -> BigInt {
    let base_fee_lower_bound = base_fee.div_floor(&BigInt::from(factor));
    if base_fee_lower_bound < *MINIMUM_BASE_FEE {
        return MINIMUM_BASE_FEE.clone();
    }
    base_fee_lower_bound
}

/// Gets the gas reward for the given message.
pub(crate) fn get_gas_reward(msg: &SignedMessage, base_fee: &BigInt) -> BigInt {
    let mut max_prem = msg.gas_fee_cap() - base_fee;
    if &max_prem < msg.gas_premium() {
        max_prem = msg.gas_premium().clone();
    }
    max_prem * msg.gas_limit()
}

pub(crate) fn get_gas_perf(gas_reward: &BigInt, gas_limit: i64) -> f64 {
    let a = BigRational::new(gas_reward * types::BLOCK_GAS_LIMIT, gas_limit.into());
    a.to_f64().unwrap()
}

/// Attempt to get a signed message that corresponds to an unsigned message in bls_sig_cache.
pub(crate) async fn recover_sig(
    bls_sig_cache: &mut LruCache<Cid, Signature>,
    msg: UnsignedMessage,
) -> Result<SignedMessage, Error> {
    let val = bls_sig_cache
        .get(&msg.cid()?)
        .ok_or_else(|| Error::Other("Could not recover sig".to_owned()))?;
    let smsg = SignedMessage::new_from_parts(msg, val.clone()).map_err(Error::Other)?;
    Ok(smsg)
}

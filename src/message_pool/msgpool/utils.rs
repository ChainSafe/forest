// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::MINIMUM_BASE_FEE;
use crate::message::{Message as MessageTrait, SignedMessage};
use crate::shim::{crypto::Signature, econ::TokenAmount, message::Message};
use cid::Cid;
use fvm_ipld_encoding::Cbor;
use lru::LruCache;
use num_rational::BigRational;
use num_traits::ToPrimitive;

use crate::message_pool::Error;

pub(in crate::message_pool) fn get_base_fee_lower_bound(
    base_fee: &TokenAmount,
    factor: i64,
) -> TokenAmount {
    let base_fee_lower_bound = base_fee.div_floor(factor);
    if base_fee_lower_bound.atto() < &MINIMUM_BASE_FEE.into() {
        TokenAmount::from_atto(MINIMUM_BASE_FEE)
    } else {
        base_fee_lower_bound
    }
}

/// Gets the gas reward for the given message.
pub(in crate::message_pool) fn get_gas_reward(
    msg: &SignedMessage,
    base_fee: &TokenAmount,
) -> TokenAmount {
    let mut max_prem = msg.gas_fee_cap() - base_fee;
    if max_prem < msg.gas_premium() {
        max_prem = msg.gas_premium();
    }
    max_prem * msg.gas_limit()
}

pub(in crate::message_pool) fn get_gas_perf(gas_reward: &TokenAmount, gas_limit: u64) -> f64 {
    let a = BigRational::new(
        gas_reward.atto() * fvm_shared::BLOCK_GAS_LIMIT,
        gas_limit.into(),
    );
    a.to_f64().unwrap()
}

/// Attempt to get a signed message that corresponds to an unsigned message in
/// `bls_sig_cache`.
pub(in crate::message_pool) fn recover_sig(
    bls_sig_cache: &mut LruCache<Cid, Signature>,
    msg: Message,
) -> Result<SignedMessage, Error> {
    let val = bls_sig_cache
        .get(&msg.cid()?)
        .ok_or_else(|| Error::Other("Could not recover sig".to_owned()))?;
    let smsg = SignedMessage::new_from_parts(msg, val.clone())?;
    Ok(smsg)
}

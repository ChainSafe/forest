// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::MINIMUM_BASE_FEE;
use crate::message::{MessageRead as _, SignedMessage};
use crate::message_pool::{
    Error,
    msgpool::{RBF_DENOM, REPLACE_BY_FEE_RATIO_MIN},
};
use crate::shim::address::Address;
use crate::shim::{crypto::Signature, econ::TokenAmount, message::Message, percent::Percent};
use crate::utils::cache::SizeTrackingCache;
use crate::utils::get_size::CidWrapper;
use ahash::HashMap;
use num_rational::BigRational;
use num_traits::ToPrimitive;

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
        gas_reward.atto() * crate::shim::econ::BLOCK_GAS_LIMIT,
        gas_limit.into(),
    );
    a.to_f64().unwrap()
}

/// Attempt to get a signed message that corresponds to an unsigned message in
/// `bls_sig_cache`.
pub(in crate::message_pool) fn recover_sig(
    bls_sig_cache: &SizeTrackingCache<CidWrapper, Signature>,
    msg: Message,
) -> Result<SignedMessage, Error> {
    let val = bls_sig_cache
        .get(&msg.cid())
        .ok_or_else(|| Error::Other("Could not recover sig".to_owned()))?;
    let smsg = SignedMessage::new_from_parts(msg, val)?;
    Ok(smsg)
}

pub(in crate::message_pool) fn add_to_selected_msgs(
    m: SignedMessage,
    rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>,
) {
    rmsgs.entry(m.from()).or_default().insert(m.sequence(), m);
}

/// Computes the minimum gas premium required to replace an existing message
/// using [`REPLACE_BY_FEE_RATIO_MIN`].
///
/// See <https://github.com/filecoin-project/lotus/blob/v1.36.0/chain/messagepool/messagepool.go#L210-L213>
pub(crate) fn compute_rbf_min_premium(premium: &TokenAmount) -> TokenAmount {
    (premium * *REPLACE_BY_FEE_RATIO_MIN).div_floor(RBF_DENOM) + TokenAmount::from_atto(1u8)
}

/// Computes the gas premium required to replace an existing message
/// using provided replace-by-fee ratio.
///
/// See <https://github.com/filecoin-project/lotus/blob/v1.36.0/chain/messagepool/messagepool.go#L215-L219>
pub(crate) fn compute_rbf(premium: &TokenAmount, replace_by_fee_ratio: Percent) -> TokenAmount {
    (premium * *replace_by_fee_ratio).div_floor(RBF_DENOM) + TokenAmount::from_atto(1u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_rbf() {
        let replace_by_fee_ratio = Percent(125);
        assert_eq!(
            super::compute_rbf(&TokenAmount::from_atto(100u64), replace_by_fee_ratio),
            TokenAmount::from_atto(126u64) // 100 * 125/100 + 1
        );
    }

    #[test]
    fn test_compute_rbf_min_premium() {
        assert_eq!(
            super::compute_rbf_min_premium(&TokenAmount::from_atto(100u64)),
            TokenAmount::from_atto(111u64) // 100 * 110/100 + 1
        );
    }
}

// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::shim::actors::power;
use crate::shim::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;
use num::{BigInt, Integer};
use num_traits::Zero;
use std::sync::Arc;

// constants for Weight calculation
/// The ratio of weight contributed by short-term vs long-term factors in a
/// given round
const W_RATIO_NUM: u64 = 1;
const W_RATIO_DEN: u64 = 2;

/// Blocks epoch allowed
const BLOCKS_PER_EPOCH: u64 = 5;

/// Returns the weight of provided [Tipset]. This function will load power actor
/// state and calculate the total weight of the [Tipset].
pub(in crate::fil_cns) fn weight<DB>(db: &Arc<DB>, ts: &Tipset) -> Result<BigInt, String>
where
    DB: Blockstore,
{
    let state_tree = StateTree::new_from_tipset(Arc::clone(db), ts).map_err(|e| e.to_string())?;
    let state: power::State = state_tree.get_actor_state().map_err(|e| e.to_string())?;

    let tpow = state.into_total_quality_adj_power();
    let log2_p = if tpow > BigInt::zero() {
        BigInt::from(tpow.bits() - 1)
    } else {
        return Err(
            "All power in the net is gone. You network might be disconnected, or the net is dead!"
                .to_owned(),
        );
    };

    let mut total_j = 0;
    for b in ts.block_headers() {
        total_j += b
            .election_proof
            .as_ref()
            .ok_or("Block contained no election proof when calculating weight")?
            .win_count;
    }

    let mut out = ts.weight().to_owned();
    out += &log2_p << 8;
    let mut e_weight: BigInt = log2_p * W_RATIO_NUM;
    e_weight <<= 8;
    e_weight *= total_j;
    e_weight = e_weight.div_floor(&(BigInt::from(BLOCKS_PER_EPOCH * W_RATIO_DEN)));
    out += &e_weight;
    Ok(out)
}

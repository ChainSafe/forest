use fil_actors_runtime_v8::fvm_ipld_amt;
use fil_actors_runtime_v8::MessageAccumulator;
use fvm_ipld_amt::Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::{address::Protocol, econ::TokenAmount};
use num_traits::Zero;

use crate::{LaneState, State};

pub struct StateSummary {
    pub redeemed: TokenAmount,
}

/// Checks internal invariants of paych state
pub fn check_state_invariants<BS: Blockstore>(
    state: &State,
    store: &BS,
    balance: &TokenAmount,
) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();
    let mut redeemed = TokenAmount::zero();

    acc.require(
        state.from.protocol() == Protocol::ID,
        format!("from address is not ID address {}", state.from),
    );
    acc.require(
        state.to.protocol() == Protocol::ID,
        format!("to address is not ID address {}", state.to),
    );
    acc.require(
        state.settling_at >= state.min_settle_height,
        format!(
            "channel is setting at epoch {} before min settle height {}",
            state.settling_at, state.min_settle_height
        ),
    );

    match Amt::<LaneState, _>::load(&state.lane_states, store) {
        Ok(lanes) => {
            let ret = lanes.for_each(|i, lane| {
                acc.require(
                    lane.redeemed.is_positive(),
                    format!(
                        "lane {i} redeemed is not greater than zero {}",
                        lane.redeemed
                    ),
                );
                redeemed += &lane.redeemed;
                Ok(())
            });
            acc.require_no_error(ret, "error iterating lanes");
        }
        Err(e) => acc.add(format!("error loading lanes: {e}")),
    }

    acc.require(
        balance >= &state.to_send,
        format!(
            "channel has insufficient funds to send ({} < {})",
            balance, state.to_send
        ),
    );

    (StateSummary { redeemed }, acc)
}

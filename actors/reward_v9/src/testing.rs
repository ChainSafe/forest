use crate::{baseline_power_from_prev, State};
use fil_actors_runtime_v9::MessageAccumulator;
use fvm_shared::{clock::ChainEpoch, econ::TokenAmount};
use num_traits::Signed;

#[derive(Default)]
pub struct StateSummary {}

pub fn check_state_invariants(
    state: &State,
    prior_epoch: ChainEpoch,
    balance: &TokenAmount,
) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();

    let storage_mining_allocation_check = TokenAmount::from_whole(1_100_000_000);

    // Can't assert equality because anyone can send funds to reward actor (and already have on mainnet)
    acc.require(
        &state.total_storage_power_reward + balance >= storage_mining_allocation_check,
        format!(
            "reward given {} + reward left {} < storage mining allocation {}",
            state.total_storage_power_reward, balance, storage_mining_allocation_check
        ),
    );

    acc.require(
        state.epoch == prior_epoch + 1,
        format!(
            "reward state epoch {} does not match prior_epoch+1 {}",
            state.epoch,
            prior_epoch + 1
        ),
    );
    acc.require(
        state.effective_network_time <= state.epoch,
        format!(
            "effective network time {} greater than state epoch {}",
            state.effective_network_time, state.epoch
        ),
    );

    acc.require(
        state.cumsum_realized <= state.cumsum_baseline,
        format!(
            "cumsum realized {} > cumsum baseline {}",
            state.cumsum_realized, state.cumsum_baseline
        ),
    );
    acc.require(
        !state.cumsum_realized.is_negative(),
        format!("cumsum realized negative ({})", state.cumsum_realized),
    );

    // Theoretically we should compare effective_baseline_power <= this_epoch_baseline_power but
    // because of rounding issues explained and tracked in https://github.com/filecoin-project/builtin-actors/issues/459
    // we settled on this workaround.
    let next_epoch_baseline_power = baseline_power_from_prev(&state.this_epoch_baseline_power);
    acc.require(
        state.effective_baseline_power <= next_epoch_baseline_power,
        format!(
            "effective baseline power ({}) > next_epoch_baseline_power ({})",
            state.effective_baseline_power, next_epoch_baseline_power
        ),
    );

    (StateSummary::default(), acc)
}

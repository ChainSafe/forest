use crate::shim::clock::ChainEpoch;
// use fil_actor_power_state::{v14::State as StateV14, v15::State as StateV15};
use fil_actor_interface::power::State;

pub trait PowerStateExt {
    fn ramp_start_epoch(&self) -> ChainEpoch;
    fn ramp_duration_epochs(&self) -> u64;
}

impl PowerStateExt for State {
    fn ramp_start_epoch(&self) -> ChainEpoch {
        match self {
            State::V15(st) => st.ramp_start_epoch,
            _ => 0,
        }
    }

    fn ramp_duration_epochs(&self) -> u64 {
        match self {
            State::V15(st) => st.ramp_duration_epochs,
            _ => 0,
        }
    }
}

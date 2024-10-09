use crate::shim::clock::ChainEpoch;
use fil_actor_interface::power::State;

pub trait PowerStateExt {
    /// FIP0081 activation epoch. Should be same as TukTuk epoch.
    fn ramp_start_epoch(&self) -> ChainEpoch;
    /// FIP0081 activation ramp. One year on mainnet, 3 days on calibnet,
    /// defaults to 200 epochs on devnet. Only applicable to v15 (aka TukTuk)
    /// actors.
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

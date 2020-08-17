// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ChannelInfo, VoucherInfo, PaychStore, Error};
use state_manager::StateManager;
use blockstore::BlockStore;
use actor::paych::{State, LaneState};
use address::Address;
use cid::Cid;
use num_bigint::BigInt;
use actor::ActorState;

struct Manager<DB> {
    store: PaychStore,
    sm: StateManager<DB>
}

impl<DB> Manager<DB>
where
    DB: BlockStore
{
    pub fn new(sm : StateManager<DB>, store: PaychStore) -> Self {
        Manager{ store, sm }
    }

    pub fn load_paych_state(&self, ch: &Address) -> Result<(ActorState, State), Error> {
        let state: State = self.sm.load_actor_state(ch, &Cid::default()).map_err(|err| Error::Other(err.to_string()))?;
        let actor = self.sm.get_actor(ch, &Cid::default()).map_err(|err| Error::Other(err.to_string()))?.ok_or_else(||Error::Other("could not find actor".to_string()))?;

        Ok((actor, state))
    }

    pub fn lane_state(&self, ch: Address, lane: u64) -> Result<LaneState, Error> {
        let (_, state) = self.load_paych_state(&ch)?;
        let ls = find_lane(state.lane_states, lane).unwrap_or(LaneState{ id: lane, redeemed: BigInt::default(), nonce: 0});
        unimplemented!()

    }

    pub fn track_inbound_channel(&mut self, ch: Address) -> Result<(), Error> {
        unimplemented!()
    }
}

fn find_lane(states: Vec<LaneState>, lane: u64) -> Option<LaneState> {
    for lane_state in states.iter() {
        if lane_state.id == lane {
            return Some(lane_state.clone())
        }
    }
    None
}

pub fn max_lane_from_state(st: &State) -> u64 {
    let mut max_lane = 0;
    for lane in st.lane_states.iter() {
        if max_lane < lane.id {
            max_lane = lane.id;
        }
    }
    max_lane
}
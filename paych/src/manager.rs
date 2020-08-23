// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ChannelInfo, Error, PaychStore};
use crate::{DIR_OUTBOUND, DIR_INBOUND, VoucherInfo};
use actor::account::State as AccountState;
use actor::paych::{LaneState, State as PaychState, SignedVoucher};
use actor::ActorState;
use address::Address;
use async_std::sync::{Arc, RwLock};
use blockstore::BlockStore;
use cid::Cid;
use state_manager::StateManager;
use num_bigint::BigInt;

struct Manager<DB> {
    store: Arc<RwLock<PaychStore>>,
    sm: Arc<RwLock<StateManager<DB>>>,
}

impl<DB> Manager<DB>
where
    DB: BlockStore,
{
    pub fn new(sm: StateManager<DB>, store: PaychStore) -> Self {
        Manager {
            store: Arc::new(RwLock::new(store)),
            sm: Arc::new(RwLock::new(sm)),
        }
    }

    pub async fn load_paych_state(&self, ch: &Address) -> Result<(ActorState, PaychState), Error> {
        let sm = self.sm.read().await;
        let state: PaychState = sm
            .load_actor_state(ch, &Cid::default())
            .map_err(|err| Error::Other(err.to_string()))?;
        let actor = sm
            .get_actor(ch, &Cid::default())
            .map_err(|err| Error::Other(err.to_string()))?
            .ok_or_else(|| Error::Other("could not find actor".to_string()))?;

        Ok((actor, state))
    }

    // TODO make sure that this is correct
    pub async fn next_lane_from_state(&self, st: PaychState) -> Result<u64, Error> {
        if st.lane_states.len() == 0 {
            return Ok(0);
        }
        let mut max_id = 0;
        for state in &st.lane_states {
            if state.id > max_id {
                max_id = state.id
            }
        }
        return Ok(max_id + 1);
    }

    pub async fn load_state_channel_info(
        &self,
        ch: Address,
        dir: u8,
    ) -> Result<ChannelInfo, Error> {
        let (_, st) = self.load_paych_state(&ch).await?;
        let sm = self.sm.read().await;
        let account_from: AccountState = sm
            .load_actor_state(&st.from, &Cid::default())
            .map_err(|err| Error::Other(err.to_string()))?;
        let from = account_from.address;
        let account_to: AccountState = sm
            .load_actor_state(&st.to, &Cid::default())
            .map_err(|err| Error::Other(err.to_string()))?;
        let to = account_to.address;
        let next_lane = self.next_lane_from_state(st).await?;
        if dir == DIR_INBOUND {
            let ci = ChannelInfo::builder()
                .next_lane(next_lane)
                .direction(dir)
                .control(from)
                .target(to)
                .build()
                .map_err(|err| Error::Other(err.to_string()))?;
            Ok(ci)
        } else if dir == DIR_OUTBOUND {
            let ci = ChannelInfo::builder()
                .next_lane(next_lane)
                .direction(dir)
                .control(to)
                .target(from)
                .build()
                .map_err(|err| Error::Other(err.to_string()))?;
            Ok(ci)
        } else {
            return Err(Error::Other("Invalid Direction".to_string()));
        }
    }

    // TODO implement channel accessor stuff after finishing paych and simple


    pub async fn track_inbound_channel(&mut self, ch: Address) -> Result<(), Error> {
        self.track_channel(ch, DIR_INBOUND).await
    }

    pub async fn track_outbound_channel(&mut self, ch: Address) -> Result<(), Error> {
        self.track_channel(ch, DIR_OUTBOUND).await
    }

    pub async fn track_channel(&mut self, ch: Address, direction: u8) -> Result<(), Error> {
        let mut store = self.store.write().await;
        let ci = self.load_state_channel_info(ch, direction).await?;
        store.track_channel(ci).await
    }

    // TODO implement when channel accessors are implemented
    pub async fn get_paych(&self, _from: Address, _to: Address, _amt: BigInt) -> Result<(Address, Cid), Error> {
        unimplemented!()
    }

    // GetPaychWaitReady waits until the create channel / add funds message with the
    // given message CID arrives.
    // The returned channel address can safely be used against the Manager methods.
    // TODO implement when channel accessors are implemented
    pub async fn get_paych_wait_ready(&self, _mcid: Cid) -> Result<Address, Error> {
        unimplemented!()
    }

    pub async fn list_channels(&self) -> Result<Vec<Address>, Error> {
        let store = self.store.read().await;
        store.list_channels().await
    }

    /// TODO implement when channel accessors are implemented
    pub async fn get_channel_info(&self, _addr: Address) -> Result<ChannelInfo, Error> {
        unimplemented!()
    }

    // TODO implement when channel accessors are implemented
    pub async fn check_voucher_valid(&self, _ch: Address, _sv: SignedVoucher) -> Result<(), Error> {
        unimplemented!()
    }

    // TODO implement when channel accessors are implemented
    pub async fn add_voucher(&self, _ch: Address, _sv: SignedVoucher, _proof: Vec<u8>, _min_delta: BigInt) -> Result<BigInt, Error> {
        unimplemented!()
    }

    // TODO implement when channel accessors are implemented
    pub async fn allocate_lane(&self, _ch: Address) -> Result<u64, Error> {
        unimplemented!()
    }

    // TODO implement when channel accessors are implemented
    pub async fn list_vouchers(&self, _ch: Address) -> Result<Vec<VoucherInfo>, Error> {
        unimplemented!()
    }

    // TODO implement when channel accessors are implemented
    pub async fn next_sequence_for_lane(&self, _ch: Address, _lane: u64) -> Result<u64, Error> {
        unimplemented!()
    }

    // TODO implement when channel accessors are implemented
    pub async fn settle(&self, _addr: Address) -> Result<Cid, Error> {
        unimplemented!()
    }

    // TODO implement when channel accessors are implemented
    pub async fn collect(&self, _addr: Address) -> Result<Cid, Error> {
        unimplemented!()
    }
}

fn find_lane(states: Vec<LaneState>, lane: u64) -> Option<LaneState> {
    for lane_state in states.iter() {
        if lane_state.id == lane {
            return Some(lane_state.clone());
        }
    }
    None
}

pub fn max_lane_from_state(st: &PaychState) -> u64 {
    let mut max_lane = 0;
    for lane in st.lane_states.iter() {
        if max_lane < lane.id {
            max_lane = lane.id;
        }
    }
    max_lane
}

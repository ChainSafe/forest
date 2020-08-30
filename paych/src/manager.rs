// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ChannelInfo, Error, PaychStore, StateAccessor};
use crate::{DIR_OUTBOUND, DIR_INBOUND, VoucherInfo, ChannelAccessor};
use actor::account::State as AccountState;
use actor::paych::{LaneState, State as PaychState, SignedVoucher};
use actor::ActorState;
use address::Address;
use async_std::sync::{Arc, RwLock};
use blockstore::BlockStore;
use cid::Cid;
use state_manager::StateManager;
use num_bigint::BigInt;
use std::collections::HashMap;

#[derive(Clone)]
pub struct Manager<DB> {
    pub store: Arc<RwLock<PaychStore>>,
    pub sa: Arc<StateAccessor<DB>>,
    pub channels: Arc<RwLock<HashMap<String, Arc<ChannelAccessor<DB>>>>>
}

impl<DB> Manager<DB>
where
DB: BlockStore
{
    pub fn new(sa: StateAccessor<DB>, store: PaychStore) -> Self {
        Manager {
            store: Arc::new(RwLock::new(store)),
            sa: Arc::new(sa),
            channels: Arc::new(RwLock::new(HashMap::new())),
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
        let ci = self.sa.load_state_channel_info(ch, direction).await?;
        store.track_channel(ci).await
    }

    pub async fn accessor_by_from_to(&self, from: Address, to: Address) -> Result<Arc<ChannelAccessor<DB>>, Error> {
        let mut channels = self.channels.write().await;
        let key = accessor_cache_key(&from, &to);

        // check if channel accessor is in cache without taking write lock
        let op = channels.get(&key);
        if let Some(channel) = op {
            return Ok(channel.clone())
        }

        // channel accessor is not in cache so take a write lock, and create new entry in cache
        let mut channel_write = self.channels.write().await;
        let ca = ChannelAccessor::new(&self);
        channel_write.insert(key.clone(), Arc::new(ca)).ok_or_else(|| Error::Other("insert new channel accesor".to_string()))?;
        let channel_check = self.channels.read().await;
        let op_locked = channel_check.get(&key);
        if let Some(channel) = op_locked {
            return Ok(channel.clone())
        }
        return Err(Error::Other("could not find channel accessor".to_owned()))
    }

    // Add a channel accessor to the cache. Note that the
    // channel may not have been created yet, but we still want to reference
    // the same channel accessor for a given from/to, so that all attempts to
    // access a channel use the same lock (the lock on the accessor)
    pub async fn add_accessor_to_cache(&self, from: Address, to: Address) -> Result<Arc<ChannelAccessor<DB>>, Error> {
        let key = accessor_cache_key(&from, &to);
        let ca = ChannelAccessor::new(&self);
        let mut channels = self.channels.write().await;
        channels.insert(key, Arc::new(ca)).ok_or_else(|| Error::Other("inserting new channel accessor".to_string()))
    }

    pub async fn accessor_by_address(&self, ch: Address) -> Result<Arc<ChannelAccessor<DB>>, Error> {
        let store = self.store.read().await;
        let ci = store.by_address(ch).await?;
        self.accessor_by_from_to(ci.control, ci.target).await
    }

    // TODO implement when channel accessors are implemented
    pub async fn get_paych(&self, from: Address, to: Address, amt: BigInt) -> Result<(Address, Cid), Error> {
        let chan_accesor = self.accessor_by_from_to(from.clone(), to.clone()).await?;
        unimplemented!()
        // return chan_accesor.get_paych(from, to, amt)
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

fn accessor_cache_key(from: &Address, to: &Address) -> String {
    from.to_string() + "->" + &to.to_string()
}

// fn find_lane(states: Vec<LaneState>, lane: u64) -> Option<LaneState> {
//     for lane_state in states.iter() {
//         if lane_state.id == lane {
//             return Some(lane_state.clone());
//         }
//     }
//     None
// }

// pub fn max_lane_from_state(st: &PaychState) -> u64 {
//     let mut max_lane = 0;
//     for lane in st.lane_states.iter() {
//         if max_lane < lane.id {
//             max_lane = lane.id;
//         }
//     }
//     max_lane
// }

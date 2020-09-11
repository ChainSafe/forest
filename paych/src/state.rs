// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Error;
use crate::{ChannelInfo, DIR_INBOUND, DIR_OUTBOUND};
use actor::account::State as AccountState;
use actor::paych::State as PaychState;
use actor::ActorState;
use address::Address;
use async_std::sync::{Arc, RwLock};
use blockstore::BlockStore;
use cid::Cid;
use ipld_amt::Amt;
use state_manager::StateManager;

pub struct StateAccessor<DB> {
    pub sm: Arc<RwLock<StateManager<DB>>>,
}

impl<DB> StateAccessor<DB>
where
    DB: BlockStore,
{
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

    pub async fn next_lane_from_state(&self, st: PaychState) -> Result<u64, Error> {
        let sm = self.sm.read().await;
        let store = sm.get_block_store_ref();
        let lane_states: Amt<u64, _> = Amt::load(&st.lane_states, store).unwrap(); // TODO handle err properly
        let mut max_id: u64 = 0;

        lane_states
            .for_each(|i: u64, _| {
                if i > max_id {
                    max_id = i
                }
                Ok(())
            })
            .unwrap(); // handle err properly

        Ok(max_id + 1)
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
                .map_err(Error::Other)?;
            Ok(ci)
        } else if dir == DIR_OUTBOUND {
            let ci = ChannelInfo::builder()
                .next_lane(next_lane)
                .direction(dir)
                .control(to)
                .target(from)
                .build()
                .map_err(Error::Other)?;
            Ok(ci)
        } else {
            Err(Error::Other("Invalid Direction".to_string()))
        }
    }
}

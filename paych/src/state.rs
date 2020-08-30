use state_manager::StateManager;
use super::{Error};
use actor::paych::State as PaychState;
use actor::ActorState;
use async_std::sync::{Arc, RwLock};
use crate::{ChannelInfo, DIR_INBOUND, DIR_OUTBOUND};
use blockstore::BlockStore;
use cid::Cid;
use address::Address;
use actor::account::State as AccountState;

pub struct StateAccessor<DB> {
    pub sm: Arc<RwLock<StateManager<DB>>>,
}

impl<DB> StateAccessor<DB>
where
DB: BlockStore
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

    // TODO make sure that this is correct
    pub async fn next_lane_from_state(&self, st: PaychState) -> Result<u64, Error> {
        unimplemented!();
        // if st.lane_states.len() == 0 {
        //     return Ok(0);
        // }
        // let mut max_id = 0;
        // for state in &st.lane_states {
        //     if state.id > max_id {
        //         max_id = state.id
        //     }
        // }
        // return Ok(max_id + 1);
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
}
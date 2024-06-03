// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::actors::convert::{
    from_policy_v13_to_v10, from_policy_v13_to_v11, from_policy_v13_to_v9,
};

use super::*;

impl MinerStateExt for State {
    fn load_sectors_ext<BS: Blockstore>(
        &self,
        store: &BS,
        sectors: Option<&BitField>,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>> {
        match self {
            State::V8(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v8::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(info.clone().into());
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V9(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v9::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(info.clone().into());
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V10(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v10::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(info.clone().into());
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V11(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v11::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(info.clone().into());
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V12(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v12::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(info.clone().into());
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V13(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v13::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(info.clone().into());
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
        }
    }

    fn find_sector<BS: Blockstore>(
        &self,
        store: &BS,
        sector_number: SectorNumber,
        policy: &fil_actors_shared::v13::runtime::policy::Policy,
    ) -> anyhow::Result<SectorLocation> {
        let (deadline, partition) = match self {
            State::V8(st) => {
                st.find_sector(&from_policy_v13_to_v9(policy), store, sector_number)?
            }
            State::V9(st) => {
                st.find_sector(&from_policy_v13_to_v9(policy), store, sector_number)?
            }
            State::V10(st) => {
                st.find_sector(&from_policy_v13_to_v10(policy), store, sector_number)?
            }
            State::V11(st) => {
                st.find_sector(&from_policy_v13_to_v11(policy), store, sector_number)?
            }
            State::V12(st) => st.find_sector(store, sector_number)?,
            State::V13(st) => st.find_sector(store, sector_number)?,
        };

        Ok(SectorLocation {
            deadline,
            partition,
        })
    }
}

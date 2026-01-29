// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::{actors::convert::*, clock::ChainEpoch, runtime::Policy};
use anyhow::Context as _;

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
            State::V14(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v14::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(info.clone().into());
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V15(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v15::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(info.clone().into());
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V16(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v16::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(info.clone().into());
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V17(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v17::Sectors::load(&store, &st.sectors)?;
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
}

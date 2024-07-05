// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context as _;

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
        }
    }

    fn load_allocated_sector_numbers<BS: Blockstore>(
        &self,
        store: &BS,
    ) -> anyhow::Result<BitField> {
        let allocated_sectors = match self {
            Self::V8(s) => s.allocated_sectors,
            Self::V9(s) => s.allocated_sectors,
            Self::V10(s) => s.allocated_sectors,
            Self::V11(s) => s.allocated_sectors,
            Self::V12(s) => s.allocated_sectors,
            Self::V13(s) => s.allocated_sectors,
            Self::V14(s) => s.allocated_sectors,
        };
        store.get_cbor_required(&allocated_sectors)
    }

    fn load_precommit_on_chain_info<BS: Blockstore>(
        &self,
        store: &BS,
        sector_number: u64,
    ) -> anyhow::Result<Option<SectorPreCommitOnChainInfo>> {
        Ok(match self {
            Self::V8(s) => s
                .get_precommitted_sector(store, sector_number)?
                .map(SectorPreCommitOnChainInfo::from),
            Self::V9(s) => s
                .get_precommitted_sector(store, sector_number)?
                .map(SectorPreCommitOnChainInfo::from),
            Self::V10(s) => s
                .get_precommitted_sector(store, sector_number)?
                .map(SectorPreCommitOnChainInfo::from),
            Self::V11(s) => s
                .get_precommitted_sector(store, sector_number)?
                .map(SectorPreCommitOnChainInfo::from),
            Self::V12(s) => s
                .get_precommitted_sector(store, sector_number)?
                .map(SectorPreCommitOnChainInfo::from),
            Self::V13(s) => s
                .get_precommitted_sector(store, sector_number)?
                .map(SectorPreCommitOnChainInfo::from),
            Self::V14(s) => s
                .get_precommitted_sector(store, sector_number)
                .context("precommit info does not exist")?
                .map(SectorPreCommitOnChainInfo::from),
        })
    }
}

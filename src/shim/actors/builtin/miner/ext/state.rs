// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::actors::{Policy, convert::*};
use anyhow::Context as _;

use crate::shim::clock::ChainEpoch;

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
            Self::V15(s) => s.allocated_sectors,
            Self::V16(s) => s.allocated_sectors,
            Self::V17(s) => s.allocated_sectors,
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
            Self::V15(s) => s
                .get_precommitted_sector(store, sector_number)
                .context("precommit info does not exist")?
                .map(SectorPreCommitOnChainInfo::from),
            Self::V16(s) => s
                .get_precommitted_sector(store, sector_number)
                .context("precommit info does not exist")?
                .map(SectorPreCommitOnChainInfo::from),
            Self::V17(s) => s
                .get_precommitted_sector(store, sector_number)
                .context("precommit info does not exist")?
                .map(SectorPreCommitOnChainInfo::from),
        })
    }

    /// Returns deadline calculations for the state recorded proving period and deadline.
    /// This is out of date if the a miner does not have an active miner cron
    fn recorded_deadline_info(&self, policy: &Policy, current_epoch: ChainEpoch) -> DeadlineInfo {
        match self {
            State::V8(st) => st
                .recorded_deadline_info(&from_policy_v13_to_v9(policy), current_epoch)
                .into(),
            State::V9(st) => st
                .recorded_deadline_info(&from_policy_v13_to_v9(policy), current_epoch)
                .into(),
            State::V10(st) => st
                .recorded_deadline_info(&from_policy_v13_to_v10(policy), current_epoch)
                .into(),
            State::V11(st) => st
                .recorded_deadline_info(&from_policy_v13_to_v11(policy), current_epoch)
                .into(),
            State::V12(st) => st
                .recorded_deadline_info(&from_policy_v13_to_v12(policy), current_epoch)
                .into(),
            State::V13(st) => st.recorded_deadline_info(policy, current_epoch).into(),
            State::V14(st) => st
                .recorded_deadline_info(&from_policy_v13_to_v14(policy), current_epoch)
                .into(),
            State::V15(st) => st
                .recorded_deadline_info(&from_policy_v13_to_v15(policy), current_epoch)
                .into(),
            State::V16(st) => st
                .recorded_deadline_info(&from_policy_v13_to_v16(policy), current_epoch)
                .into(),
            State::V17(st) => st
                .recorded_deadline_info(&from_policy_v13_to_v17(policy), current_epoch)
                .into(),
        }
    }
}

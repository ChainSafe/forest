// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::errors::*;
use crate::StateManager;
use actor::miner;
use actor::{
    miner::{ChainSectorInfo, Deadlines, MinerInfo, SectorOnChainInfo, SectorPreCommitOnChainInfo},
    power,
};
use address::{Address, Protocol};
use bitfield::BitField;
use blockstore::BlockStore;
use cid::Cid;
use encoding::serde_bytes::ByteBuf;
use fil_types::{RegisteredSealProof, SectorInfo, SectorNumber, SectorSize, HAMT_BIT_WIDTH};
use filecoin_proofs_api::{post::generate_winning_post_sector_challenge, ProverId};
use forest_blocks::Tipset;
use ipld_amt::Amt;
use ipld_hamt::Hamt;
use std::convert::TryInto;

pub fn get_sectors_for_winning_post<DB>(
    state_manager: &StateManager<DB>,
    st: &Cid,
    address: &Address,
    rand: &[u8; 32],
) -> Result<Vec<SectorInfo>, Error>
where
    DB: BlockStore,
{
    let miner_actor_state: miner::State =
        state_manager
            .load_actor_state(&address, &st)
            .map_err(|err| {
                Error::State(format!(
                    "(get sectors) failed to load miner actor state: %{:}",
                    err
                ))
            })?;
    let sector_set = get_proving_set_raw(state_manager, &miner_actor_state)?;
    if sector_set.is_empty() {
        return Ok(Vec::new());
    }
    let seal_proof_type = match miner_actor_state.info.sector_size {
        SectorSize::_2KiB => RegisteredSealProof::StackedDRG2KiBV1,
        SectorSize::_8MiB => RegisteredSealProof::StackedDRG8MiBV1,
        SectorSize::_512MiB => RegisteredSealProof::StackedDRG512MiBV1,
        SectorSize::_32GiB => RegisteredSealProof::StackedDRG32GiBV1,
        SectorSize::_64GiB => RegisteredSealProof::StackedDRG64GiBV1,
    };
    let wpt = seal_proof_type.registered_winning_post_proof()?;

    if address.protocol() != Protocol::ID {
        return Err(Error::Other(format!(
            "failed to get ID from miner address {:}",
            address
        )));
    };
    let mut prover_id = ProverId::default();
    let prover_bytes = address.to_bytes();
    prover_id[..prover_bytes.len()].copy_from_slice(&prover_bytes);
    let ids = generate_winning_post_sector_challenge(
        wpt.try_into()?,
        &rand,
        sector_set.len() as u64,
        prover_id,
    )
    .map_err(|err| Error::State(format!("generate winning posts challenge {:}", err)))?;

    Ok(ids
        .iter()
        .map::<Result<SectorInfo, Error>, _>(|i: &u64| {
            let index = *i as usize;
            let sector_number = sector_set
                .get(index)
                .ok_or_else(|| {
                    Error::Other(format!("Could not get sector_number at index {:}", index))
                })?
                .info
                .sector_number;
            let sealed_cid = sector_set
                .get(index)
                .ok_or_else(|| {
                    Error::Other(format!("Could not get sealed cid at index {:}", index))
                })?
                .info
                .sealed_cid
                .clone();
            Ok(SectorInfo {
                proof: seal_proof_type,
                sector_number,
                sealed_cid,
            })
        })
        .collect::<Result<Vec<SectorInfo>, _>>()?)
}

pub fn get_proving_set_raw<DB>(
    state_manager: &StateManager<DB>,
    actor_state: &miner::State,
) -> Result<Vec<miner::SectorOnChainInfo>, Error>
where
    DB: BlockStore,
{
    let not_proving = &actor_state.faults | &actor_state.recoveries;

    actor_state
        .load_sector_infos(&*state_manager.get_block_store(), &not_proving)
        .map_err(|err| Error::Other(format!("failed to get proving set :{:}", err)))
}

pub fn get_miner_sector_set<DB>(
    state_manager: &StateManager<DB>,
    tipset: &Tipset,
    address: &Address,
    filter: &mut Option<&mut BitField>,
    filter_out: bool,
) -> Result<Vec<ChainSectorInfo>, Error>
where
    DB: BlockStore,
{
    let miner_actor_state: miner::State = state_manager
        .load_actor_state(&address, &tipset.parent_state())
        .map_err(|err| {
            Error::State(format!(
                "(get miner sector set) failed to load miner actor state: {:}",
                err
            ))
        })?;
    load_sectors_from_set(
        state_manager.get_block_store_ref(),
        &miner_actor_state.sectors,
        filter,
        filter_out,
    )
}

fn load_sectors_from_set<DB>(
    block_store: &DB,
    ssc: &Cid,
    filter: &mut Option<&mut BitField>,
    _filter_out: bool,
) -> Result<Vec<ChainSectorInfo>, Error>
where
    DB: BlockStore,
{
    let amt = Amt::load(ssc, block_store).map_err(|err| Error::Other(err.to_string()))?;

    let mut sset: Vec<ChainSectorInfo> = Vec::new();
    let for_each = |i: u64, sector_chain: &miner::SectorOnChainInfo| {
        if let Some(ref mut s) = filter {
            let i = i
                .try_into()
                .map_err(|_| "Could not convert from index to usize")?;
            if s.get(i) {
                return Ok(());
            }
        }
        sset.push(ChainSectorInfo {
            info: sector_chain.info.to_owned(),
            id: i,
        });
        Ok(())
    };
    amt.for_each(for_each)
        .map_err(|err| Error::Other(format!("Error Processing ForEach {:}", err)))?;

    Ok(sset)
}

pub fn miner_sector_info<DB>(
    state_manager: &StateManager<DB>,
    address: &Address,
    sector_number: &SectorNumber,
    tipset: &Tipset,
) -> Result<Option<SectorOnChainInfo>, Error>
where
    DB: BlockStore,
{
    let miner_actor_state: miner::State = state_manager
        .load_actor_state(&address, &tipset.parent_state())
        .map_err(|err| {
            Error::State(format!(
                "(get miner sector info) failed to load miner actor state: {:}",
                err
            ))
        })?;
    miner_actor_state
        .get_sector(state_manager.get_block_store_ref(), *sector_number)
        .map_err(|err| Error::State(format!("(get sset) failed to get actor state: {:}", err)))
}

pub fn precommit_info<DB>(
    state_manager: &StateManager<DB>,
    address: &Address,
    sector_number: &SectorNumber,
    tipset: &Tipset,
) -> Result<SectorPreCommitOnChainInfo, Error>
where
    DB: BlockStore,
{
    let miner_actor_state: miner::State = state_manager
        .load_actor_state(&address, &tipset.parent_state())
        .map_err(|err| {
            Error::State(format!(
                "(get precommit info) failed to load miner actor state: {:}",
                err
            ))
        })?;
    let precommit_info = miner_actor_state
        .get_precommitted_sector(state_manager.get_block_store_ref(), *sector_number)
        .map_err(|err| {
            Error::Other(format!(
                "(precommit info) failed to load miner actor state: %{:}",
                err
            ))
        })?;
    Ok(precommit_info.ok_or_else(|| Error::Other("precommit not found".to_string()))?)
}

pub fn get_miner_info<DB>(
    state_manager: &StateManager<DB>,
    tipset: &Tipset,
    address: &Address,
) -> Result<MinerInfo, Error>
where
    DB: BlockStore,
{
    let miner_actor_state: miner::State = state_manager
        .load_actor_state(&address, &tipset.parent_state())
        .map_err(|err| {
            Error::State(format!(
                "(get miner info) failed to load miner actor state: {:}",
                err
            ))
        })?;
    Ok(miner_actor_state.info)
}

pub fn get_miner_deadlines<DB>(
    state_manager: &StateManager<DB>,
    tipset: &Tipset,
    address: &Address,
) -> Result<Deadlines, Error>
where
    DB: BlockStore,
{
    let miner_actor_state: miner::State = state_manager
        .load_actor_state(&address, &tipset.parent_state())
        .map_err(|err| {
            Error::State(format!(
                "(get miner deadlines) failed to load miner actor state: {:}",
                err
            ))
        })?;
    miner_actor_state
        .load_deadlines(&*state_manager.get_block_store())
        .map_err(|err| {
            Error::State(format!(
                "(get_miner_deadlines) could not load deadlines: {:}",
                err
            ))
        })
}

pub fn get_miner_faults<DB>(
    state_manager: &StateManager<DB>,
    tipset: &Tipset,
    address: &Address,
) -> Result<BitField, Error>
where
    DB: BlockStore,
{
    let miner_actor_state: miner::State = state_manager
        .load_actor_state(&address, &tipset.parent_state())
        .map_err(|err| {
            Error::State(format!(
                "(get miner faults) failed to load miner actor state: {:}",
                err
            ))
        })?;
    Ok(miner_actor_state.faults)
}

pub fn get_miner_recoveries<DB>(
    state_manager: &StateManager<DB>,
    tipset: &Tipset,
    address: &Address,
) -> Result<BitField, Error>
where
    DB: BlockStore,
{
    let miner_actor_state: miner::State = state_manager
        .load_actor_state(&address, &tipset.parent_state())
        .map_err(|err| {
            Error::State(format!(
                "(get miner recoveries) failed to load miner actor state: {:}",
                err
            ))
        })?;
    Ok(miner_actor_state.recoveries)
}

pub fn list_miner_actors<'a, DB>(
    state_manager: &'a StateManager<DB>,
    tipset: &'a Tipset,
) -> Result<Vec<Address>, Error>
where
    DB: BlockStore,
{
    let power_actor_state: power::State = state_manager
        .load_actor_state(&actor::STORAGE_POWER_ACTOR_ADDR, &tipset.parent_state())
        .map_err(|err| {
            Error::State(format!(
                "(get sset) failed to load power actor state: {:}",
                err
            ))
        })?;
    let mut miners: Vec<Address> = Vec::new();
    let block_store = &*state_manager.get_block_store();
    let map =
        Hamt::<_, _>::load_with_bit_width(&power_actor_state.claims, block_store, HAMT_BIT_WIDTH)
            .map_err(|err| Error::Other(err.to_string()))?;
    map.for_each(|_, k: &ByteBuf| {
        let address = Address::from_bytes(k.as_ref())?;
        miners.push(address);
        Ok(())
    })
    .map_err(|e| Error::Other(e.to_string()))?;
    Ok(miners)
}

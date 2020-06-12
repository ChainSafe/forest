// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::errors::*;
use crate::StateManager;
use actor::miner::State;
use address::{Address, Payload};
use blockstore::BlockStore;
use cid::Cid;
use fil_types::{RegisteredProof, SectorInfo, SectorSize};
use filecoin_proofs_api::{post::generate_winning_post_sector_challenge, ProverId};
use std::convert::TryInto;

pub fn get_sectors_winning_for_winning_post<DB>(
    state_manager: &StateManager<DB>,
    st: &Cid,
    address: &Address,
    rand: &[u8; 32],
) -> Result<Vec<SectorInfo>, Error>
where
    DB: BlockStore,
{
    let miner_actor_state: State = state_manager.load_actor_state(&address, &st)?;
    let mut not_proving = miner_actor_state
        .faults
        .clone()
        .merge(&miner_actor_state.recoveries)
        .map_err(|_| Error::Other("Could not merge bitfield".to_string()))?;
    let sector_set =
        miner_actor_state.load_sector_infos(&*state_manager.get_block_store(), &mut not_proving)?;
    if sector_set.is_empty() {
        return Ok(Vec::new());
    }
    let seal_proof_type = match miner_actor_state.info.sector_size {
        SectorSize::_2KiB => RegisteredProof::StackedDRG2KiBSeal,
        SectorSize::_8MiB => RegisteredProof::StackedDRG8MiBSeal,
        SectorSize::_512MiB => RegisteredProof::StackedDRG512MiBSeal,
        SectorSize::_32GiB => RegisteredProof::StackedDRG32GiBSeal,
        SectorSize::_64GiB => RegisteredProof::StackedDRG64GiBSeal,
    };
    let wpt = seal_proof_type.registered_winning_post_proof()?;

    let mid = match address.payload() {
        Payload::ID(new_id) => Address::new_id(*new_id),
        _ => return Err(Error::Other(format!("getting miner id {:}", address))),
    };
    let mut prover_id = ProverId::default();
    let prover_bytes = mid.to_bytes();
    prover_id[..prover_bytes.len()].copy_from_slice(&prover_bytes);
    let ids = generate_winning_post_sector_challenge(
        wpt.into(),
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
                proof: wpt,
                sector_number,
                sealed_cid,
            })
        })
        .collect::<Result<Vec<SectorInfo>, _>>()?)
}

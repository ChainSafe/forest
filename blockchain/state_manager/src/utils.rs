// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::errors::*;
use crate::StateManager;
use actor::miner;
use address::{Address, Protocol};
use blockstore::BlockStore;
use cid::Cid;
use fil_types::{RegisteredProof, SectorInfo, SectorSize};
use filecoin_proofs_api::{post::generate_winning_post_sector_challenge, ProverId};

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
        SectorSize::_2KiB => RegisteredProof::StackedDRG2KiBSeal,
        SectorSize::_8MiB => RegisteredProof::StackedDRG8MiBSeal,
        SectorSize::_512MiB => RegisteredProof::StackedDRG512MiBSeal,
        SectorSize::_32GiB => RegisteredProof::StackedDRG32GiBSeal,
        SectorSize::_64GiB => RegisteredProof::StackedDRG64GiBSeal,
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

fn get_proving_set_raw<DB>(
    state_manager: &StateManager<DB>,
    actor_state: &miner::State,
) -> Result<Vec<miner::SectorOnChainInfo>, Error>
where
    DB: BlockStore,
{
    let mut not_proving = actor_state
        .faults
        .clone()
        .merge(&actor_state.recoveries)
        .map_err(|_| Error::Other("Could not merge bitfield".to_string()))?;

    actor_state
        .load_sector_infos(&*state_manager.get_block_store(), &mut not_proving)
        .map_err(|err| Error::Other(format!("failed to get proving set :{:}", err)))
}

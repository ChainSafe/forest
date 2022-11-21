// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::Cbor;
use fvm_shared::address::Address;
use fvm_shared::bigint::bigint_ser::BigIntDe;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::error::ExitCode;
use fvm_shared::piece::PaddedPieceSize;
use fvm_shared::sector::SectorNumber;
use fvm_shared::{ActorID, HAMT_BIT_WIDTH};

use fil_actors_runtime_v9::{
    actor_error, make_empty_map, make_map_with_root_and_bitwidth, ActorError, AsActorError, Map,
    MapMap,
};

use crate::DataCap;
use crate::{AllocationID, ClaimID};

#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone)]
pub struct State {
    pub root_key: Address,
    // Maps verifier addresses to data cap minting allowance (in bytes).
    pub verifiers: Cid, // HAMT[Address]DataCap
    pub remove_data_cap_proposal_ids: Cid,
    // Maps client IDs to allocations made by that client.
    pub allocations: Cid, // HAMT[ActorID]HAMT[AllocationID]Allocation
    // Next allocation identifier to use.
    // The value 0 is reserved to mean "no allocation".
    pub next_allocation_id: u64,
    // Maps provider IDs to allocations claimed by that provider.
    pub claims: Cid, // HAMT[ActorID]HAMT[ClaimID]Claim
}

impl State {
    pub fn new<BS: Blockstore>(store: &BS, root_key: Address) -> Result<State, ActorError> {
        let empty_map = make_empty_map::<_, ()>(store, HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| actor_error!(illegal_state, "failed to create empty map: {}", e))?;

        let empty_mapmap =
            MapMap::<_, (), ActorID, u64>::new(store, HAMT_BIT_WIDTH, HAMT_BIT_WIDTH)
                .flush()
                .map_err(|e| {
                    actor_error!(illegal_state, "failed to create empty multi map: {}", e)
                })?;

        Ok(State {
            root_key,
            verifiers: empty_map,
            remove_data_cap_proposal_ids: empty_map,
            allocations: empty_mapmap,
            next_allocation_id: 1,
            claims: empty_mapmap,
        })
    }

    // Adds a verifier and cap, overwriting any existing cap for that verifier.
    pub fn put_verifier(
        &mut self,
        store: &impl Blockstore,
        verifier: &Address,
        cap: &DataCap,
    ) -> Result<(), ActorError> {
        let mut verifiers =
            make_map_with_root_and_bitwidth::<_, BigIntDe>(&self.verifiers, store, HAMT_BIT_WIDTH)
                .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to load verifiers")?;
        verifiers
            .set(verifier.to_bytes().into(), BigIntDe(cap.clone()))
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to set verifier")?;
        self.verifiers = verifiers
            .flush()
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to flush verifiers")?;
        Ok(())
    }

    pub fn remove_verifier(
        &mut self,
        store: &impl Blockstore,
        verifier: &Address,
    ) -> Result<(), ActorError> {
        let mut verifiers =
            make_map_with_root_and_bitwidth::<_, BigIntDe>(&self.verifiers, store, HAMT_BIT_WIDTH)
                .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to load verifiers")?;

        verifiers
            .delete(&verifier.to_bytes())
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to remove verifier")?
            .context_code(ExitCode::USR_ILLEGAL_ARGUMENT, "verifier not found")?;

        self.verifiers = verifiers
            .flush()
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to flush verifiers")?;
        Ok(())
    }

    pub fn get_verifier_cap(
        &self,
        store: &impl Blockstore,
        verifier: &Address,
    ) -> Result<Option<DataCap>, ActorError> {
        let verifiers =
            make_map_with_root_and_bitwidth::<_, BigIntDe>(&self.verifiers, store, HAMT_BIT_WIDTH)
                .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to load verifiers")?;
        let allowance = verifiers
            .get(&verifier.to_bytes())
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to get verifier")?;
        Ok(allowance.map(|a| a.0.clone() as DataCap))
    }

    pub fn load_verifiers<'a, BS: Blockstore>(
        &self,
        store: &'a BS,
    ) -> Result<Map<'a, BS, BigIntDe>, ActorError> {
        make_map_with_root_and_bitwidth::<_, BigIntDe>(&self.verifiers, store, HAMT_BIT_WIDTH)
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to load verifiers")
    }

    pub fn load_allocs<'a, BS: Blockstore>(
        &self,
        store: &'a BS,
    ) -> Result<MapMap<'a, BS, Allocation, ActorID, AllocationID>, ActorError> {
        MapMap::<BS, Allocation, ActorID, AllocationID>::from_root(
            store,
            &self.allocations,
            HAMT_BIT_WIDTH,
            HAMT_BIT_WIDTH,
        )
        .context_code(
            ExitCode::USR_ILLEGAL_STATE,
            "failed to load allocations table",
        )
    }

    pub fn save_allocs<'a, BS: Blockstore>(
        &mut self,
        allocs: &mut MapMap<'a, BS, Allocation, ActorID, AllocationID>,
    ) -> Result<(), ActorError> {
        self.allocations = allocs.flush().context_code(
            ExitCode::USR_ILLEGAL_STATE,
            "failed to flush allocations table",
        )?;
        Ok(())
    }

    /// Inserts a batch of allocations under a single client address.
    /// The allocations are assigned sequential IDs starting from the next available.
    pub fn insert_allocations<BS: Blockstore, I>(
        &mut self,
        store: &BS,
        client: ActorID,
        new_allocs: I,
    ) -> Result<Vec<AllocationID>, ActorError>
    where
        I: Iterator<Item = Allocation>,
    {
        let mut allocs = self.load_allocs(store)?;
        // These local variables allow the id-associating map closure to move the allocations
        // from the iterator rather than clone, without moving self.
        let first_id = self.next_allocation_id;
        let mut count = 0;
        let count_ref = &mut count;
        allocs
            .put_many(
                client,
                new_allocs.map(move |a| {
                    let id = first_id + *count_ref;
                    *count_ref += 1;
                    (id, a)
                }),
            )
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to put allocations")?;
        self.save_allocs(&mut allocs)?;
        self.next_allocation_id += count;
        let allocated_ids = (first_id..first_id + count).collect();
        Ok(allocated_ids)
    }

    pub fn load_claims<'a, BS: Blockstore>(
        &self,
        store: &'a BS,
    ) -> Result<MapMap<'a, BS, Claim, ActorID, ClaimID>, ActorError> {
        MapMap::<BS, Claim, ActorID, ClaimID>::from_root(
            store,
            &self.claims,
            HAMT_BIT_WIDTH,
            HAMT_BIT_WIDTH,
        )
        .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to load claims table")
    }

    pub fn save_claims<'a, BS: Blockstore>(
        &mut self,
        claims: &mut MapMap<'a, BS, Claim, ActorID, ClaimID>,
    ) -> Result<(), ActorError> {
        self.claims = claims
            .flush()
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to flush claims table")?;
        Ok(())
    }

    pub fn put_claims<BS: Blockstore, I>(&mut self, store: &BS, claims: I) -> Result<(), ActorError>
    where
        I: Iterator<Item = (ClaimID, Claim)>,
    {
        let mut st_claims = self.load_claims(store)?;
        for (id, claim) in claims {
            st_claims
                .put(claim.provider, id, claim)
                .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to put claim")?;
        }
        self.save_claims(&mut st_claims)?;
        Ok(())
    }
}
#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    // The provider storing the data (from allocation).
    pub provider: ActorID,
    // The client which allocated the DataCap (from allocation).
    pub client: ActorID,
    // Identifier of the data committed (from allocation).
    pub data: Cid,
    // The (padded) size of data (from allocation).
    pub size: PaddedPieceSize,
    // The min period after term_start which the provider must commit to storing data
    pub term_min: ChainEpoch,
    // The max period after term_start for which provider can earn QA-power for the data
    pub term_max: ChainEpoch,
    // The epoch at which the (first range of the) piece was committed.
    pub term_start: ChainEpoch,
    // ID of the provider's sector in which the data is committed.
    pub sector: SectorNumber,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug, PartialEq, Eq)]
pub struct Allocation {
    // The verified client which allocated the DataCap.
    pub client: ActorID,
    // The provider (miner actor) which may claim the allocation.
    pub provider: ActorID,
    // Identifier of the data to be committed.
    pub data: Cid,
    // The (padded) size of data.
    pub size: PaddedPieceSize,
    // The minimum duration which the provider must commit to storing the piece to avoid
    // early-termination penalties (epochs).
    pub term_min: ChainEpoch,
    // The maximum period for which a provider can earn quality-adjusted power
    // for the piece (epochs).
    pub term_max: ChainEpoch,
    // The latest epoch by which a provider must commit data before the allocation expires.
    pub expiration: ChainEpoch,
}

impl Cbor for State {}

pub fn get_allocation<'a, BS>(
    allocations: &'a mut MapMap<BS, Allocation, ActorID, AllocationID>,
    client: ActorID,
    id: AllocationID,
) -> Result<Option<&'a Allocation>, ActorError>
where
    BS: Blockstore,
{
    allocations.get(client, id).context_code(
        ExitCode::USR_ILLEGAL_STATE,
        "HAMT lookup failure getting allocation",
    )
}

pub fn get_claim<'a, BS>(
    claims: &'a mut MapMap<BS, Claim, ActorID, ClaimID>,
    provider: ActorID,
    id: ClaimID,
) -> Result<Option<&'a Claim>, ActorError>
where
    BS: Blockstore,
{
    claims.get(provider, id).context_code(
        ExitCode::USR_ILLEGAL_STATE,
        "HAMT lookup failure getting claim",
    )
}

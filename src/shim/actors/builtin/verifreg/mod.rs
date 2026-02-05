// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod ext;
use crate::shim::address::{Address, Protocol};
use anyhow::anyhow;
use cid::Cid;
use fil_actor_verifreg_state::v13::ClaimID;
use fil_actor_verifreg_state::{
    v9::state::get_claim as get_claim_v9, v10::state::get_claim as get_claim_v10,
    v11::state::get_claim as get_claim_v11, v12::state::get_claim as get_claim_v12,
    v13::state::get_claim as get_claim_v13, v14::state::get_claim as get_claim_v14,
    v15::state::get_claim as get_claim_v15, v16::state::get_claim as get_claim_v16,
    v17::state::get_claim as get_claim_v17,
};
use fil_actors_shared::v8::{HAMT_BIT_WIDTH, make_map_with_root_and_bitwidth};
use fil_actors_shared::v9::Keyer;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_shared4::ActorID;
use fvm_shared4::bigint::bigint_ser::BigIntDe;
use fvm_shared4::clock::ChainEpoch;
use fvm_shared4::piece::PaddedPieceSize;
use fvm_shared4::sector::SectorNumber;
use num::BigInt;
use serde::{Deserialize, Serialize};
use spire_enum::prelude::delegated_enum;

/// verifreg actor address.
pub const ADDRESS: Address = Address::new_id(6);

/// Verifreg actor state.
#[delegated_enum(impl_conversions)]
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_verifreg_state::v8::State),
    V9(fil_actor_verifreg_state::v9::State),
    V10(fil_actor_verifreg_state::v10::State),
    V11(fil_actor_verifreg_state::v11::State),
    V12(fil_actor_verifreg_state::v12::State),
    V13(fil_actor_verifreg_state::v13::State),
    V14(fil_actor_verifreg_state::v14::State),
    V15(fil_actor_verifreg_state::v15::State),
    V16(fil_actor_verifreg_state::v16::State),
    V17(fil_actor_verifreg_state::v17::State),
}

impl State {
    pub fn default_latest_version(
        root_key: fvm_shared4::address::Address,
        verifiers: Cid,
        remove_data_cap_proposal_ids: Cid,
        allocations: Cid,
        next_allocation_id: u64,
        claims: Cid,
    ) -> Self {
        State::V17(fil_actor_verifreg_state::v17::State {
            root_key,
            verifiers,
            remove_data_cap_proposal_ids,
            allocations,
            next_allocation_id,
            claims,
        })
    }

    pub fn verified_client_data_cap<BS>(
        &self,
        store: &BS,
        addr: Address,
    ) -> anyhow::Result<Option<BigInt>>
    where
        BS: Blockstore,
    {
        if addr.protocol() != Protocol::ID {
            return Err(anyhow!("can only look up ID addresses"));
        }

        match self {
            State::V8(state) => {
                let vh = make_map_with_root_and_bitwidth(
                    &state.verified_clients,
                    store,
                    HAMT_BIT_WIDTH,
                )?;
                Ok(vh
                    .get(&fvm_shared2::address::Address::from(addr).key())?
                    .map(|int: &BigIntDe| int.0.to_owned()))
            }
            _ => Err(anyhow!("not supported in actors > v8")),
        }
    }

    pub fn verifier_data_cap<BS>(&self, store: &BS, addr: Address) -> anyhow::Result<Option<BigInt>>
    where
        BS: Blockstore,
    {
        if addr.protocol() != Protocol::ID {
            return Err(anyhow!("can only look up ID addresses"));
        }

        let key = fvm_shared2::address::Address::from(addr).key();
        delegate_state!(self => |st| {
            let vh = make_map_with_root_and_bitwidth(&st.verifiers, store, HAMT_BIT_WIDTH)?;
            Ok(vh.get(&key)?.map(|int: &BigIntDe| int.0.to_owned()))
        })
    }

    pub fn get_allocation<BS>(
        &self,
        store: &BS,
        addr: ActorID,
        allocation_id: AllocationID,
    ) -> anyhow::Result<Option<Allocation>>
    where
        BS: Blockstore,
    {
        match self {
            State::V8(_) => Err(anyhow!("unsupported in actors v8")),
            State::V9(state) => {
                let mut map = state.load_allocs(store)?;
                Ok(fil_actor_verifreg_state::v9::state::get_allocation(
                    &mut map,
                    addr,
                    allocation_id,
                )?
                .map(Allocation::from))
            }
            State::V10(state) => {
                let mut map = state.load_allocs(store)?;
                Ok(fil_actor_verifreg_state::v10::state::get_allocation(
                    &mut map,
                    addr,
                    allocation_id,
                )?
                .map(Allocation::from))
            }
            State::V11(state) => {
                let mut map = state.load_allocs(store)?;
                Ok(fil_actor_verifreg_state::v11::state::get_allocation(
                    &mut map,
                    addr,
                    allocation_id,
                )?
                .map(Allocation::from))
            }
            State::V12(state) => {
                let mut map = state.load_allocs(store)?;
                Ok(fil_actor_verifreg_state::v12::state::get_allocation(
                    &mut map,
                    addr,
                    allocation_id,
                )?
                .map(Allocation::from))
            }
            State::V13(state) => {
                let mut map = state.load_allocs(store)?;
                Ok(fil_actor_verifreg_state::v13::state::get_allocation(
                    &mut map,
                    addr,
                    allocation_id,
                )?
                .map(Allocation::from))
            }
            State::V14(state) => {
                let mut map = state.load_allocs(store)?;
                Ok(fil_actor_verifreg_state::v14::state::get_allocation(
                    &mut map,
                    addr,
                    allocation_id,
                )?
                .map(Allocation::from))
            }
            State::V15(state) => {
                let mut map = state.load_allocs(store)?;
                Ok(fil_actor_verifreg_state::v15::state::get_allocation(
                    &mut map,
                    addr,
                    allocation_id,
                )?
                .map(Allocation::from))
            }
            State::V16(state) => {
                let mut map = state.load_allocs(store)?;
                Ok(fil_actor_verifreg_state::v16::state::get_allocation(
                    &mut map,
                    addr,
                    allocation_id,
                )?
                .map(Allocation::from))
            }
            State::V17(state) => {
                let mut map = state.load_allocs(store)?;
                Ok(fil_actor_verifreg_state::v17::state::get_allocation(
                    &mut map,
                    addr,
                    allocation_id,
                )?
                .map(Allocation::from))
            }
        }
    }

    pub fn get_claim<BS>(
        &self,
        store: &BS,
        addr: Address,
        claim_id: ClaimID,
    ) -> anyhow::Result<Option<Claim>>
    where
        BS: Blockstore,
    {
        let provider_id = addr.id()?;

        match self {
            State::V8(_) => Err(anyhow!("unsupported in actors v8")),
            State::V9(state) => {
                Ok(
                    get_claim_v9(&mut state.load_claims(store)?, provider_id, claim_id)?
                        .map(Claim::from),
                )
            }
            State::V10(state) => {
                Ok(
                    get_claim_v10(&mut state.load_claims(store)?, provider_id, claim_id)?
                        .map(Claim::from),
                )
            }
            State::V11(state) => {
                Ok(
                    get_claim_v11(&mut state.load_claims(store)?, provider_id, claim_id)?
                        .map(Claim::from),
                )
            }
            State::V12(state) => {
                Ok(
                    get_claim_v12(&mut state.load_claims(store)?, provider_id, claim_id)?
                        .map(Claim::from),
                )
            }
            State::V13(state) => {
                Ok(
                    get_claim_v13(&mut state.load_claims(store)?, provider_id, claim_id)?
                        .map(Claim::from),
                )
            }
            State::V14(state) => {
                Ok(
                    get_claim_v14(&mut state.load_claims(store)?, provider_id, claim_id)?
                        .map(Claim::from),
                )
            }
            State::V15(state) => {
                Ok(
                    get_claim_v15(&mut state.load_claims(store)?, provider_id, claim_id)?
                        .map(Claim::from),
                )
            }
            State::V16(state) => {
                Ok(
                    get_claim_v16(&mut state.load_claims(store)?, provider_id, claim_id)?
                        .map(Claim::from),
                )
            }
            State::V17(state) => {
                Ok(
                    get_claim_v17(&mut state.load_claims(store)?, provider_id, claim_id)?
                        .map(Claim::from),
                )
            }
        }
    }

    /// Returns the root key address for this verifreg state.
    pub fn root_key(&self) -> Address {
        delegate_state!(self.root_key.into())
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

macro_rules! from_claim {
    ($($type:ty),* $(,)*) => {
        $(
        impl From<&$type> for Claim {
            fn from(claim: &$type) -> Self {
                Self {
                    provider: claim.provider,
                    client: claim.client,
                    data: claim.data,
                    size: PaddedPieceSize(claim.size.0),
                    term_min: claim.term_min,
                    term_max: claim.term_max,
                    term_start: claim.term_start,
                    sector: claim.sector,
                }
            }
        }
        )*
    };
}

from_claim!(
    fil_actor_verifreg_state::v17::Claim,
    fil_actor_verifreg_state::v16::Claim,
    fil_actor_verifreg_state::v15::Claim,
    fil_actor_verifreg_state::v14::Claim,
    fil_actor_verifreg_state::v13::Claim,
    fil_actor_verifreg_state::v12::Claim,
    fil_actor_verifreg_state::v11::Claim,
    fil_actor_verifreg_state::v10::Claim,
    fil_actor_verifreg_state::v9::Claim,
);

pub type AllocationID = u64;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
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

macro_rules! from_allocation {
    ($type: ty) => {
        impl From<&$type> for Allocation {
            fn from(alloc: &$type) -> Self {
                Self {
                    client: alloc.client,
                    provider: alloc.provider,
                    data: alloc.data,
                    size: PaddedPieceSize(alloc.size.0),
                    term_min: alloc.term_min,
                    term_max: alloc.term_max,
                    expiration: alloc.expiration,
                }
            }
        }
    };
}

from_allocation!(fil_actor_verifreg_state::v17::Allocation);
from_allocation!(fil_actor_verifreg_state::v16::Allocation);
from_allocation!(fil_actor_verifreg_state::v15::Allocation);
from_allocation!(fil_actor_verifreg_state::v14::Allocation);
from_allocation!(fil_actor_verifreg_state::v13::Allocation);
from_allocation!(fil_actor_verifreg_state::v12::Allocation);
from_allocation!(fil_actor_verifreg_state::v11::Allocation);
from_allocation!(fil_actor_verifreg_state::v10::Allocation);
from_allocation!(fil_actor_verifreg_state::v9::Allocation);

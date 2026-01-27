// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use anyhow::Context as _;
macro_rules! list_all_inner_pre_v12 {
    ($state:ident, $store:ident, $version:ident, $method:ident, $map:ident) => {{
        let mut entities = $state.$method($store)?;
        let mut actors = vec![];
        entities.for_each_outer(|k, _| {
            let actor_id = fil_actors_shared::$version::parse_uint_key(k)?;
            actors.push(actor_id);
            Ok(())
        })?;

        for actor_id in actors {
            entities.for_each(actor_id, |k, v| {
                let claim_id = fil_actors_shared::$version::parse_uint_key(k)?;
                $map.insert(claim_id, v.into());
                Ok(())
            })?;
        }
    }};
}

macro_rules! list_all_inner {
    ($state:ident, $store:ident, $version:ident, $method:ident, $map:ident) => {{
        let mut entities = $state.$method($store)?;
        let mut actors = vec![];
        entities.for_each(|k, _| {
            let actor_id = fil_actors_shared::$version::parse_uint_key(k)?;
            actors.push(actor_id);
            Ok(())
        })?;

        for actor_id in actors {
            entities.for_each_in(actor_id, |k, v| {
                let claim_id = fil_actors_shared::$version::parse_uint_key(k)?;
                $map.insert(claim_id, v.into());
                Ok(())
            })?;
        }
    }};
}

impl VerifiedRegistryStateExt for State {
    fn get_allocations<BS: Blockstore>(
        &self,
        store: &BS,
        address: &Address,
    ) -> anyhow::Result<HashMap<AllocationID, Allocation>> {
        let address_id = address.id().context("can only look up ID addresses")?;
        let mut result = HashMap::default();
        match self {
            State::V8(_) => return Err(anyhow::anyhow!("unsupported in actors v8")),
            State::V9(state) => {
                let mut map = state.load_allocs(store)?;
                map.for_each(address_id, |k, v| {
                    let allocation_id = fil_actors_shared::v9::parse_uint_key(k)?;
                    result.insert(allocation_id, v.into());
                    Ok(())
                })?;
            }
            State::V10(state) => {
                let mut map = state.load_allocs(store)?;
                map.for_each(address_id, |k, v| {
                    let allocation_id = fil_actors_shared::v10::parse_uint_key(k)?;
                    result.insert(allocation_id, v.into());
                    Ok(())
                })?;
            }
            State::V11(state) => {
                let mut map = state.load_allocs(store)?;
                map.for_each(address_id, |k, v| {
                    let allocation_id = fil_actors_shared::v11::parse_uint_key(k)?;
                    result.insert(allocation_id, v.into());
                    Ok(())
                })?;
            }
            State::V12(state) => {
                let mut map = state.load_allocs(store)?;
                map.for_each_in(address_id, |k, v| {
                    let allocation_id = fil_actors_shared::v12::parse_uint_key(k)?;
                    result.insert(allocation_id, v.into());
                    Ok(())
                })?;
            }
            State::V13(state) => {
                let mut map = state.load_allocs(store)?;
                map.for_each_in(address_id, |k, v| {
                    let allocation_id = fil_actors_shared::v13::parse_uint_key(k)?;
                    result.insert(allocation_id, v.into());
                    Ok(())
                })?;
            }
            State::V14(state) => {
                let mut map = state.load_allocs(store)?;
                map.for_each_in(address_id, |k, v| {
                    let allocation_id = fil_actors_shared::v14::parse_uint_key(k)?;
                    result.insert(allocation_id, v.into());
                    Ok(())
                })?;
            }
            State::V15(state) => {
                let mut map = state.load_allocs(store)?;
                map.for_each_in(address_id, |k, v| {
                    let allocation_id = fil_actors_shared::v15::parse_uint_key(k)?;
                    result.insert(allocation_id, v.into());
                    Ok(())
                })?;
            }
            State::V16(state) => {
                let mut map = state.load_allocs(store)?;
                map.for_each_in(address_id, |k, v| {
                    let allocation_id = fil_actors_shared::v16::parse_uint_key(k)?;
                    result.insert(allocation_id, v.into());
                    Ok(())
                })?;
            }
            State::V17(state) => {
                let mut map = state.load_allocs(store)?;
                map.for_each_in(address_id, |k, v| {
                    let allocation_id = fil_actors_shared::v17::parse_uint_key(k)?;
                    result.insert(allocation_id, v.into());
                    Ok(())
                })?;
            }
        };
        Ok(result)
    }

    fn get_all_allocations<BS: Blockstore>(
        &self,
        store: &BS,
    ) -> anyhow::Result<HashMap<AllocationID, Allocation>> {
        let mut result = HashMap::default();
        match self {
            State::V8(_) => return Err(anyhow::anyhow!("unsupported in actors v8")),
            State::V9(state) => list_all_inner_pre_v12!(state, store, v9, load_allocs, result),
            State::V10(state) => list_all_inner_pre_v12!(state, store, v10, load_allocs, result),
            State::V11(state) => list_all_inner_pre_v12!(state, store, v11, load_allocs, result),
            State::V12(state) => list_all_inner!(state, store, v12, load_allocs, result),
            State::V13(state) => list_all_inner!(state, store, v13, load_allocs, result),
            State::V14(state) => list_all_inner!(state, store, v14, load_allocs, result),
            State::V15(state) => list_all_inner!(state, store, v15, load_allocs, result),
            State::V16(state) => list_all_inner!(state, store, v16, load_allocs, result),
            State::V17(state) => list_all_inner!(state, store, v17, load_allocs, result),
        };
        Ok(result)
    }

    fn get_claims<BS: Blockstore>(
        &self,
        store: &BS,
        provider_id_address: &Address,
    ) -> anyhow::Result<HashMap<ClaimID, Claim>> {
        let provider_id = provider_id_address
            .id()
            .context("can only look up ID addresses")?;
        let mut result = HashMap::default();
        match self {
            Self::V8(_) => return Err(anyhow::anyhow!("unsupported in actors v8")),
            Self::V9(s) => {
                let mut claims = s.load_claims(store)?;
                claims.for_each(provider_id, |k, v| {
                    let claim_id = fil_actors_shared::v9::parse_uint_key(k)?;
                    result.insert(claim_id, v.into());
                    Ok(())
                })?;
            }
            Self::V10(s) => {
                let mut claims = s.load_claims(store)?;
                claims.for_each(provider_id, |k, v| {
                    let claim_id = fil_actors_shared::v10::parse_uint_key(k)?;
                    result.insert(claim_id, v.into());
                    Ok(())
                })?;
            }
            Self::V11(s) => {
                let mut claims = s.load_claims(store)?;
                claims.for_each(provider_id, |k, v| {
                    let claim_id = fil_actors_shared::v11::parse_uint_key(k)?;
                    result.insert(claim_id, v.into());
                    Ok(())
                })?;
            }
            Self::V12(s) => {
                let mut claims = s.load_claims(store)?;
                claims.for_each_in(provider_id, |k, v| {
                    let claim_id = fil_actors_shared::v12::parse_uint_key(k)?;
                    result.insert(claim_id, v.into());
                    Ok(())
                })?;
            }
            Self::V13(s) => {
                let mut claims = s.load_claims(store)?;
                claims.for_each_in(provider_id, |k, v| {
                    let claim_id = fil_actors_shared::v13::parse_uint_key(k)?;
                    result.insert(claim_id, v.into());
                    Ok(())
                })?;
            }
            Self::V14(s) => {
                let mut claims = s.load_claims(store)?;
                claims.for_each_in(provider_id, |k, v| {
                    let claim_id = fil_actors_shared::v14::parse_uint_key(k)?;
                    result.insert(claim_id, v.into());
                    Ok(())
                })?;
            }
            Self::V15(s) => {
                let mut claims = s.load_claims(store)?;
                claims.for_each_in(provider_id, |k, v| {
                    let claim_id = fil_actors_shared::v15::parse_uint_key(k)?;
                    result.insert(claim_id, v.into());
                    Ok(())
                })?;
            }
            Self::V16(s) => {
                let mut claims = s.load_claims(store)?;
                claims.for_each_in(provider_id, |k, v| {
                    let claim_id = fil_actors_shared::v16::parse_uint_key(k)?;
                    result.insert(claim_id, v.into());
                    Ok(())
                })?;
            }
            Self::V17(s) => {
                let mut claims = s.load_claims(store)?;
                claims.for_each_in(provider_id, |k, v| {
                    let claim_id = fil_actors_shared::v17::parse_uint_key(k)?;
                    result.insert(claim_id, v.into());
                    Ok(())
                })?;
            }
        };
        Ok(result)
    }

    fn get_all_claims<BS: Blockstore>(
        &self,
        store: &BS,
    ) -> anyhow::Result<HashMap<ClaimID, Claim>> {
        let mut result = HashMap::default();
        match self {
            Self::V8(_) => return Err(anyhow::anyhow!("unsupported in actors v8")),
            State::V9(state) => list_all_inner_pre_v12!(state, store, v9, load_claims, result),
            State::V10(state) => list_all_inner_pre_v12!(state, store, v10, load_claims, result),
            State::V11(state) => list_all_inner_pre_v12!(state, store, v11, load_claims, result),
            State::V12(state) => list_all_inner!(state, store, v12, load_claims, result),
            State::V13(state) => list_all_inner!(state, store, v13, load_claims, result),
            State::V14(state) => list_all_inner!(state, store, v14, load_claims, result),
            State::V15(state) => list_all_inner!(state, store, v15, load_claims, result),
            State::V16(state) => list_all_inner!(state, store, v16, load_claims, result),
            State::V17(state) => list_all_inner!(state, store, v17, load_claims, result),
        };
        Ok(result)
    }
}

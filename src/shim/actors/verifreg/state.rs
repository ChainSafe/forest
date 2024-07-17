// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use anyhow::Context as _;

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
            State::V9(state) => {
                let mut map = state.load_allocs(store)?;
                let mut actors = vec![];
                map.for_each_outer(|k, _| {
                    let actor_id = fil_actors_shared::v9::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    map.for_each(actor_id, |k, v| {
                        let allocation_id = fil_actors_shared::v9::parse_uint_key(k)?;
                        result.insert(allocation_id, v.into());
                        Ok(())
                    })?;
                }
            }
            State::V10(state) => {
                let mut map = state.load_allocs(store)?;
                let mut actors = vec![];
                map.for_each_outer(|k, _| {
                    let actor_id = fil_actors_shared::v10::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    map.for_each(actor_id, |k, v| {
                        let allocation_id = fil_actors_shared::v10::parse_uint_key(k)?;
                        result.insert(allocation_id, v.into());
                        Ok(())
                    })?;
                }
            }
            State::V11(state) => {
                let mut map = state.load_allocs(store)?;
                let mut actors = vec![];
                map.for_each_outer(|k, _| {
                    let actor_id = fil_actors_shared::v11::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    map.for_each(actor_id, |k, v| {
                        let allocation_id = fil_actors_shared::v11::parse_uint_key(k)?;
                        result.insert(allocation_id, v.into());
                        Ok(())
                    })?;
                }
            }
            State::V12(state) => {
                let mut map = state.load_allocs(store)?;
                let mut actors = vec![];
                map.for_each(|k, _| {
                    let actor_id = fil_actors_shared::v12::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    map.for_each_in(actor_id, |k, v| {
                        let allocation_id = fil_actors_shared::v12::parse_uint_key(k)?;
                        result.insert(allocation_id, v.into());
                        Ok(())
                    })?;
                }
            }
            State::V13(state) => {
                let mut map = state.load_allocs(store)?;
                let mut actors = vec![];
                map.for_each(|k, _| {
                    let actor_id = fil_actors_shared::v13::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    map.for_each_in(actor_id, |k, v| {
                        let allocation_id = fil_actors_shared::v13::parse_uint_key(k)?;
                        result.insert(allocation_id, v.into());
                        Ok(())
                    })?;
                }
            }
            State::V14(state) => {
                let mut map = state.load_allocs(store)?;
                let mut actors = vec![];
                map.for_each(|k, _| {
                    let actor_id = fil_actors_shared::v14::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    map.for_each_in(actor_id, |k, v| {
                        let allocation_id = fil_actors_shared::v14::parse_uint_key(k)?;
                        result.insert(allocation_id, v.into());
                        Ok(())
                    })?;
                }
            }
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
            Self::V9(s) => {
                let mut claims = s.load_claims(store)?;
                let mut actors = vec![];
                claims.for_each_outer(|k, _| {
                    let actor_id = fil_actors_shared::v9::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    claims.for_each(actor_id, |k, v| {
                        let claim_id = fil_actors_shared::v9::parse_uint_key(k)?;
                        result.insert(claim_id, v.into());
                        Ok(())
                    })?;
                }
            }
            Self::V10(s) => {
                let mut claims = s.load_claims(store)?;
                let mut actors = vec![];
                claims.for_each_outer(|k, _| {
                    let actor_id = fil_actors_shared::v10::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    claims.for_each(actor_id, |k, v| {
                        let claim_id = fil_actors_shared::v10::parse_uint_key(k)?;
                        result.insert(claim_id, v.into());
                        Ok(())
                    })?;
                }
            }
            Self::V11(s) => {
                let mut claims = s.load_claims(store)?;
                let mut actors = vec![];
                claims.for_each_outer(|k, _| {
                    let actor_id = fil_actors_shared::v11::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    claims.for_each(actor_id, |k, v| {
                        let claim_id = fil_actors_shared::v11::parse_uint_key(k)?;
                        result.insert(claim_id, v.into());
                        Ok(())
                    })?;
                }
            }
            Self::V12(s) => {
                let mut claims = s.load_claims(store)?;
                let mut actors = vec![];
                claims.for_each(|k, _| {
                    let actor_id = fil_actors_shared::v12::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    claims.for_each_in(actor_id, |k, v| {
                        let claim_id = fil_actors_shared::v12::parse_uint_key(k)?;
                        result.insert(claim_id, v.into());
                        Ok(())
                    })?;
                }
            }
            Self::V13(s) => {
                let mut claims = s.load_claims(store)?;
                let mut actors = vec![];
                claims.for_each(|k, _| {
                    let actor_id = fil_actors_shared::v13::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    claims.for_each_in(actor_id, |k, v| {
                        let claim_id = fil_actors_shared::v13::parse_uint_key(k)?;
                        result.insert(claim_id, v.into());
                        Ok(())
                    })?;
                }
            }
            Self::V14(s) => {
                let mut claims = s.load_claims(store)?;
                let mut actors = vec![];
                claims.for_each(|k, _| {
                    let actor_id = fil_actors_shared::v13::parse_uint_key(k)?;
                    actors.push(actor_id);
                    Ok(())
                })?;

                for actor_id in actors {
                    claims.for_each_in(actor_id, |k, v| {
                        let claim_id = fil_actors_shared::v13::parse_uint_key(k)?;
                        result.insert(claim_id, v.into());
                        Ok(())
                    })?;
                }
            }
        };
        Ok(result)
    }
}

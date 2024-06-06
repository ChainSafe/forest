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
        };
        Ok(result)
    }
}

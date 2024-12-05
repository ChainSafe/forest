// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl MarketStateExt for market::State {
    fn get_allocations_for_pending_deals(
        &self,
        store: &impl Blockstore,
    ) -> anyhow::Result<HashMap<DealID, AllocationID>> {
        let mut result = HashMap::default();
        match self {
            Self::V8(_) => {
                anyhow::bail!("unsupported before actors v9");
            }
            Self::V9(s) => {
                let map = fil_actors_shared::v9::Map::<_, AllocationID>::load_with_bit_width(
                    &s.pending_deal_allocation_ids,
                    store,
                    fil_actors_shared::v9::HAMT_BIT_WIDTH,
                )?;
                map.for_each(|k, &v| {
                    let deal_id = fil_actors_shared::v9::parse_uint_key(k)?;
                    result.insert(deal_id, v);
                    Ok(())
                })?;
            }
            Self::V10(s) => {
                let map = fil_actors_shared::v10::Map::<_, AllocationID>::load_with_bit_width(
                    &s.pending_deal_allocation_ids,
                    store,
                    fil_actors_shared::v10::HAMT_BIT_WIDTH,
                )?;
                map.for_each(|k, &v| {
                    let deal_id = fil_actors_shared::v10::parse_uint_key(k)?;
                    result.insert(deal_id, v);
                    Ok(())
                })?;
            }
            Self::V11(s) => {
                let map = fil_actors_shared::v11::Map::<_, AllocationID>::load_with_bit_width(
                    &s.pending_deal_allocation_ids,
                    store,
                    fil_actors_shared::v11::HAMT_BIT_WIDTH,
                )?;
                map.for_each(|k, &v| {
                    let deal_id = fil_actors_shared::v11::parse_uint_key(k)?;
                    result.insert(deal_id, v);
                    Ok(())
                })?;
            }
            Self::V12(s) => {
                let map = fil_actors_shared::v12::Map::<_, AllocationID>::load_with_bit_width(
                    &s.pending_deal_allocation_ids,
                    store,
                    fil_actors_shared::v12::HAMT_BIT_WIDTH,
                )?;
                map.for_each(|k, &v| {
                    let deal_id = fil_actors_shared::v12::parse_uint_key(k)?;
                    result.insert(deal_id, v);
                    Ok(())
                })?;
            }
            Self::V13(s) => {
                let map = fil_actors_shared::v13::Map::<_, AllocationID>::load_with_bit_width(
                    &s.pending_deal_allocation_ids,
                    store,
                    fil_actors_shared::v13::HAMT_BIT_WIDTH,
                )?;
                map.for_each(|k, &v| {
                    let deal_id = fil_actors_shared::v13::parse_uint_key(k)?;
                    result.insert(deal_id, v);
                    Ok(())
                })?;
            }
            Self::V14(s) => {
                let map = fil_actors_shared::v14::Map::<_, AllocationID>::load_with_bit_width(
                    &s.pending_deal_allocation_ids,
                    store,
                    fil_actors_shared::v14::HAMT_BIT_WIDTH,
                )?;
                map.for_each(|k, &v| {
                    let deal_id = fil_actors_shared::v14::parse_uint_key(k)?;
                    result.insert(deal_id, v);
                    Ok(())
                })?;
            }
            Self::V15(s) => {
                let map = fil_actors_shared::v15::Map::<_, AllocationID>::load_with_bit_width(
                    &s.pending_deal_allocation_ids,
                    store,
                    fil_actors_shared::v15::HAMT_BIT_WIDTH,
                )?;
                map.for_each(|k, &v| {
                    let deal_id = fil_actors_shared::v15::parse_uint_key(k)?;
                    result.insert(deal_id, v);
                    Ok(())
                })?;
            }
            Self::V16(s) => {
                let map = fil_actors_shared::v16::Map::<_, AllocationID>::load_with_bit_width(
                    &s.pending_deal_allocation_ids,
                    store,
                    fil_actors_shared::v16::HAMT_BIT_WIDTH,
                )?;
                map.for_each(|k, &v| {
                    let deal_id = fil_actors_shared::v16::parse_uint_key(k)?;
                    result.insert(deal_id, v);
                    Ok(())
                })?;
            }
        }
        Ok(result)
    }

    fn get_allocation_id_for_pending_deal(
        &self,
        store: &impl Blockstore,
        deal_id: &DealID,
    ) -> anyhow::Result<AllocationID> {
        let allocations = self.get_allocations_for_pending_deals(store)?;
        Ok(allocations
            .get(deal_id)
            .copied()
            .unwrap_or(fil_actor_market_state::v14::NO_ALLOCATION_ID))
    }
}

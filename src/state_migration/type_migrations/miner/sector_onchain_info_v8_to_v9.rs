// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_miner_state::{
    v8::SectorOnChainInfo as SectorOnChainInfoV8, v9::SectorOnChainInfo as SectorOnChainInfoV9,
};
use fvm_ipld_blockstore::Blockstore;
use num::BigInt;

use super::super::super::common::{TypeMigration, TypeMigrator};

impl TypeMigration<SectorOnChainInfoV8, SectorOnChainInfoV9> for TypeMigrator {
    fn migrate_type(
        from: SectorOnChainInfoV8,
        _: &impl Blockstore,
    ) -> anyhow::Result<SectorOnChainInfoV9> {
        let big_zero = BigInt::default();

        let out_info = SectorOnChainInfoV9 {
            simple_qa_power: from.deal_weight == big_zero && from.verified_deal_weight == big_zero,
            sector_number: from.sector_number,
            seal_proof: from.seal_proof,
            sealed_cid: from.sealed_cid,
            deal_ids: from.deal_ids,
            activation: from.activation,
            expiration: from.expiration,
            deal_weight: from.deal_weight,
            verified_deal_weight: from.verified_deal_weight,
            initial_pledge: from.initial_pledge,
            expected_day_reward: from.expected_day_reward,
            expected_storage_pledge: from.expected_storage_pledge,
            replaced_sector_age: from.replaced_sector_age,
            replaced_day_reward: from.replaced_day_reward,
            sector_key_cid: from.sector_key_cid,
        };

        Ok(out_info)
    }
}

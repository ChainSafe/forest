// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    shim::{econ::TokenAmount, sector::RegisteredSealProof},
    state_migration::common::{TypeMigration, TypeMigrator},
};
use fil_actor_miner_state::{
    v11::SectorOnChainInfo as SectorOnChainInfoV11,
    v12::{SectorOnChainInfo as SectorOnChainInfoV12, SectorOnChainInfoFlags},
};
use fvm_ipld_blockstore::Blockstore;

impl TypeMigration<SectorOnChainInfoV11, SectorOnChainInfoV12> for TypeMigrator {
    fn migrate_type(
        from: SectorOnChainInfoV11,
        _: &impl Blockstore,
    ) -> anyhow::Result<SectorOnChainInfoV12> {
        let power_base_epoch = from.activation;

        let activation = if from.sector_key_cid.is_some() {
            from.activation - from.replaced_sector_age
        } else {
            from.activation
        };

        let flags = if from.simple_qa_power {
            SectorOnChainInfoFlags::SIMPLE_QA_POWER
        } else {
            SectorOnChainInfoFlags::default()
        };

        let out_info = SectorOnChainInfoV12 {
            sector_number: from.sector_number,
            seal_proof: RegisteredSealProof::from(from.seal_proof).into(),
            sealed_cid: from.sealed_cid,
            deal_ids: from.deal_ids,
            activation,
            expiration: from.expiration,
            deal_weight: from.deal_weight,
            verified_deal_weight: from.verified_deal_weight,
            initial_pledge: TokenAmount::from(from.initial_pledge).into(),
            expected_day_reward: TokenAmount::from(from.expected_day_reward).into(),
            expected_storage_pledge: TokenAmount::from(from.expected_storage_pledge).into(),
            power_base_epoch,
            replaced_day_reward: TokenAmount::from(from.replaced_day_reward).into(),
            sector_key_cid: from.sector_key_cid,
            flags,
        };

        Ok(out_info)
    }
}

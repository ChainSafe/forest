// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::StoragePower;

/// Creates state decode params tests for the Storage Power actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let power_create_miner_params = fil_actor_power_state::v16::CreateMinerParams {
        owner: Address::new_id(1000).into(),
        worker: Address::new_id(1001).into(),
        window_post_proof_type: fvm_shared4::sector::RegisteredPoStProof::StackedDRGWindow32GiBV1P1,
        peer: b"miner".to_vec(),
        multiaddrs: Default::default(),
    };

    // not supported by the lotus
    // let _power_miner_power_exp_params = fil_actor_power_state::v16::MinerPowerParams{
    //     miner: 1234,
    // };

    let power_update_claim_params = fil_actor_power_state::v16::UpdateClaimedPowerParams {
        raw_byte_delta: StoragePower::from(1024u64),
        quality_adjusted_delta: StoragePower::from(2048u64),
    };

    let power_enroll_event_params = fil_actor_power_state::v16::EnrollCronEventParams {
        event_epoch: 123,
        payload: Default::default(),
    };

    let power_update_pledge_ttl_params = fil_actor_power_state::v16::UpdatePledgeTotalParams {
        pledge_delta: Default::default(),
    };

    let power_miner_raw_params = fil_actor_power_state::v16::MinerRawPowerParams { miner: 1234 };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::CreateMiner as u64,
            to_vec(&power_create_miner_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::UpdateClaimedPower as u64,
            to_vec(&power_update_claim_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::EnrollCronEvent as u64,
            to_vec(&power_enroll_event_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::UpdatePledgeTotal as u64,
            to_vec(&power_update_pledge_ttl_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::CreateMinerExported as u64,
            to_vec(&power_create_miner_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::MinerRawPowerExported as u64,
            to_vec(&power_miner_raw_params)?,
            tipset.key().into(),
        ))?),
        // Not supported by the lotus,
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/401
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::POWER_ACTOR,
        //     Method::MinerPowerExported as u64,
        //     to_vec(&power_miner_power_exp_params)?,
        //     tipset.key().into(),
        // ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::Constructor as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::OnEpochTickEnd as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::CurrentTotalPower as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::NetworkRawPowerExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::MinerCountExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            fil_actor_power_state::v16::Method::MinerConsensusCountExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
    ])
}

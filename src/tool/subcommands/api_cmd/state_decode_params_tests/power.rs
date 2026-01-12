// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::StoragePower;
use fil_actor_power_state::v17::*;

/// Creates state decode params tests for the Storage Power actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let power_create_miner_params = CreateMinerParams {
        owner: Address::new_id(1000).into(),
        worker: Address::new_id(1001).into(),
        window_post_proof_type: fvm_shared4::sector::RegisteredPoStProof::StackedDRGWindow32GiBV1P1,
        peer: b"miner".to_vec(),
        multiaddrs: Default::default(),
    };

    // not supported by the lotus
    // let _power_miner_power_exp_params = MinerPowerParams{
    //     miner: 1234,
    // };

    let power_update_claim_params = UpdateClaimedPowerParams {
        raw_byte_delta: StoragePower::from(1024u64),
        quality_adjusted_delta: StoragePower::from(2048u64),
    };

    let power_enroll_event_params = EnrollCronEventParams {
        event_epoch: 123,
        payload: Default::default(),
    };

    let power_update_pledge_ttl_params = UpdatePledgeTotalParams {
        pledge_delta: Default::default(),
    };

    let power_miner_raw_params = MinerRawPowerParams { miner: 1234 };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::CreateMiner as u64,
            to_vec(&power_create_miner_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::UpdateClaimedPower as u64,
            to_vec(&power_update_claim_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::EnrollCronEvent as u64,
            to_vec(&power_enroll_event_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::UpdatePledgeTotal as u64,
            to_vec(&power_update_pledge_ttl_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::CreateMinerExported as u64,
            to_vec(&power_create_miner_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::MinerRawPowerExported as u64,
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
            Method::Constructor as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::OnEpochTickEnd as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::CurrentTotalPower as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::NetworkRawPowerExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::MinerCountExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::POWER_ACTOR,
            Method::MinerConsensusCountExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
    ])
}

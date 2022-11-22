// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::clock::{ChainEpoch, QuantSpec};
use fvm_shared::deal::DealID;
use fvm_shared::METHOD_CONSTRUCTOR;
use num_derive::FromPrimitive;

use fil_actors_runtime_v8::runtime::Policy;

pub use self::deal::*;
pub use self::state::*;
pub use self::types::*;

pub mod balance_table;
// export for testing
mod deal;
// export for testing
pub mod policy;
// export for testing
mod state;
mod types;

/// Market actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AddBalance = 2,
    WithdrawBalance = 3,
    PublishStorageDeals = 4,
    VerifyDealsForActivation = 5,
    ActivateDeals = 6,
    OnMinerSectorsTerminate = 7,
    ComputeDataCommitment = 8,
    CronTick = 9,
}

pub fn gen_rand_next_epoch(
    policy: &Policy,
    start_epoch: ChainEpoch,
    deal_id: DealID,
) -> ChainEpoch {
    let offset = deal_id as i64 % policy.deal_updates_interval;
    let q = QuantSpec {
        unit: policy.deal_updates_interval,
        offset: 0,
    };
    let prev_day = q.quantize_down(start_epoch);
    if prev_day + offset >= start_epoch {
        return prev_day + offset;
    }
    let next_day = q.quantize_up(start_epoch);
    next_day + offset
}

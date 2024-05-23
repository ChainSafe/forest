// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

use crate::rpc::types::MsigVesting;
use fil_actor_interface::multisig::State;

pub trait MultisigExt {
    fn get_vesting_schedule(&self) -> anyhow::Result<MsigVesting>;
}

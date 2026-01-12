// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

use crate::rpc::types::MsigVesting;
use crate::shim::actors::multisig::State;

pub trait MultisigExt {
    fn get_vesting_schedule(&self) -> anyhow::Result<MsigVesting>;
}

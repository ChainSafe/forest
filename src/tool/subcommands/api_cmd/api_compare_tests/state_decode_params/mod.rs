// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! State decode params tests for various actors.
//! 
//! This module contains test functions for verifying StateDecodeParams API functionality
//! across different actor types in the Filecoin network.

use crate::blocks::Tipset;
use anyhow::Result;

use super::RpcTest;

// Module declarations for each actor
mod account;
mod datacap;
mod eam;
mod evm;
mod init;
mod market;
mod miner;
mod multisig;
mod paych;
mod power;
mod reward;
mod verified_reg;

/// Creates all state decode params tests for all supported actors.
/// 
/// This function aggregates test cases from all actor-specific modules
/// and includes some basic system-level tests.
pub fn create_all_state_decode_params_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let mut tests = Vec::new();
    
    // TODO: Add system and cron actor tests here when implemented
    
    // Actor-specific tests
    tests.extend(account::create_tests(tipset)?);
    tests.extend(datacap::create_tests(tipset)?);
    tests.extend(eam::create_tests(tipset)?);
    tests.extend(evm::create_tests(tipset)?);
    tests.extend(init::create_tests(tipset)?);
    tests.extend(market::create_tests(tipset)?);
    tests.extend(miner::create_tests(tipset)?);
    tests.extend(multisig::create_tests(tipset)?);
    tests.extend(paych::create_tests(tipset)?);
    tests.extend(power::create_tests(tipset)?);
    tests.extend(reward::create_tests(tipset)?);
    tests.extend(verified_reg::create_tests(tipset)?);
    
    Ok(tests)
}

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![recursion_limit = "1024"]
#![allow(
    deprecated,
    unused,
    clippy::upper_case_acronyms,
    clippy::enum_variant_names,
    clippy::module_inception
)] // # 2991

mod auth;
mod beacon;
mod blocks;
mod chain;
mod chain_sync;
mod cli_shared;
mod db;
mod deleg_cns;
mod fil_cns;
mod genesis;
mod interpreter;
mod ipld;
mod json;
mod key_management;
mod libp2p;
mod libp2p_bitswap;
mod message;
mod message_pool;
mod metrics;
mod networks;
mod rpc;
mod rpc_api;
mod rpc_client;
mod shim;
mod state_manager;
mod state_migration;
mod statediff;
mod test_utils;
mod utils;

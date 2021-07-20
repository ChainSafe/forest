// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use structopt::StructOpt;

use super::{print_rpc_res, print_rpc_res_cids, print_rpc_res_pretty};
use cid::{json::CidJson, Cid};
use rpc_client::chain_ops::*;

#[derive(Debug, StructOpt)]
pub enum MpoolCommands {}

impl MpoolCommands {
    pub async fn run(&self) {}
}

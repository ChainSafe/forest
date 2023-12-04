// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::Error as JsonRpcError;

// EthAccounts will always return [] since we don't expect Forest to manage private keys
pub(in crate::rpc) async fn eth_accounts<DB: Blockstore>() -> Result<String, JsonRpcError> {
    Ok("[]".to_string())
}

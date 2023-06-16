// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::*;
use crate::beacon::Beacon;
use crate::rpc_api::{auth_api::*, data_types::RPCState};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

/// RPC call to create a new JWT Token
pub(in crate::rpc) async fn auth_new<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<AuthNewParams>,
) -> Result<AuthNewResult, JsonRpcError>
where
    DB: Blockstore,
    B: Beacon,
{
    let auth_params: AuthNewParams = params;
    let ks = data.keystore.read().await;
    let ki = ks.get(JWT_IDENTIFIER)?;
    let token = create_token(auth_params.perms, ki.private_key(), auth_params.token_exp)?;
    Ok(token.as_bytes().to_vec())
}

/// RPC call to verify JWT Token and return the token's permissions
pub(in crate::rpc) async fn auth_verify<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<AuthVerifyParams>,
) -> Result<AuthVerifyResult, JsonRpcError>
where
    DB: Blockstore,
    B: Beacon,
{
    let ks = data.keystore.read().await;
    let (header_raw,) = params;
    let token = header_raw.trim_start_matches("Bearer ");
    let ki = ks.get(JWT_IDENTIFIER)?;
    let perms = verify_token(token, ki.private_key())?;
    Ok(perms)
}

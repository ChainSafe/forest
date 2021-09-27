// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

use auth::*;
use beacon::Beacon;
use blockstore::BlockStore;
use rpc_api::{auth_api::*, data_types::RPCState};

/// RPC call to create a new JWT Token
pub(crate) async fn auth_new<'a, DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<AuthNewParams>,
) -> Result<AuthNewResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (perms,) = params;
    let ks = data.keystore.read().await;
    let ki = ks.get(JWT_IDENTIFIER)?;
    let token = create_token(perms, ki.private_key())?;
    Ok(token.as_bytes().to_vec())
}

/// RPC call to verify JWT Token and return the token's permissions
pub(crate) async fn auth_verify<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<AuthVerifyParams>,
) -> Result<AuthVerifyResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let ks = data.keystore.read().await;
    let (header_raw,) = params;
    let token = header_raw.trim_start_matches("Bearer ");
    let ki = ks.get(JWT_IDENTIFIER)?;
    let perms = verify_token(&token, ki.private_key())?;
    Ok(perms)
}

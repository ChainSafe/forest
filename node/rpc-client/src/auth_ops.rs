use crate::call_params;
use jsonrpc_v2::Error;
use rpc_api::auth_api::*;

pub async fn auth_new(perm: AuthNewParams) -> Result<AuthNewResult, Error> {
    call_params(AUTH_NEW, perm).await
}

use crate::call_params;
use jsonrpc_v2::Error;
use rpc_api::chain_api::*;

pub async fn chain_get_block(cid: ChainGetBlockParams) -> Result<ChainGetBlockResult, Error> {
    call_params(CHAIN_GET_BLOCK, cid).await
}

pub async fn chain_get_genesis() -> Result<ChainGetGenesisResult, Error> {
    call_params(CHAIN_GET_GENESIS, ()).await
}

pub async fn chain_head() -> Result<ChainHeadResult, Error> {
    call_params(CHAIN_HEAD, ()).await
}

pub async fn chain_get_message(cid: ChainGetMessageParams) -> Result<ChainGetMessageResult, Error> {
    call_params(CHAIN_GET_MESSAGE, cid).await
}

pub async fn chain_read_obj(cid: ChainReadObjParams) -> Result<ChainReadObjResult, Error> {
    call_params(CHAIN_READ_OBJ, cid).await
}

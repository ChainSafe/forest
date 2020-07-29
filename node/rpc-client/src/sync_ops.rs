



use super::client::Filecoin;
use cid::{json::CidJson, Cid};
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient as HTC;
use jsonrpc_v2::Error as JsonRpcError;
use rpc::RPCSyncState;



pub async fn mark_bad(mut client: RawClient<HTC>, block_cid: String) -> Result<(), JsonRpcError>{
        let valid_cid = Cid::from_raw_cid(block_cid)?;
        Ok(Filecoin::mark_bad(&mut client, CidJson(valid_cid)).await?)
}


pub async fn check_bad(mut client: RawClient<HTC>, block_cid: String) -> Result<String, JsonRpcError>{
    let valid_cid = Cid::from_raw_cid(block_cid)?;
    Ok(Filecoin::check_bad(&mut client, CidJson(valid_cid)).await?)
}

pub async fn status(mut client: RawClient<HTC>) -> Result<RPCSyncState, JsonRpcError>{
    Ok(Filecoin::status(&mut client).await?)
}
use super::client::Filecoin;
use crypto::signature::json::signature_type::SignatureTypeJson;
use jsonrpc_v2::Error as JsonRpcError;
use jsonrpsee::{raw::RawClient, transport::http::HttpTransportClient};

pub async fn wallet_new(
    client: &mut RawClient<HttpTransportClient>,
    signature_type: SignatureTypeJson,
) -> Result<String, JsonRpcError> {
    Ok(Filecoin::wallet_new(client, signature_type).await?)
}

pub async fn wallet_default_address(
    client: &mut RawClient<HttpTransportClient>,
) -> Result<String, JsonRpcError> {
    Ok(Filecoin::wallet_has(client).await?)
}

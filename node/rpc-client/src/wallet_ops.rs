use crypto::signature::json::signature_type::SignatureTypeJson;
use jsonrpc_v2::Error as JsonRpcError;
use jsonrpsee::{raw::RawClient, transport::http::HttpTransportClient};

pub async fn wallet_new(
    client: &mut RawClient<HttpTransportClient>,
    signature_type: SignatureTypeJson,
) -> Result<String, JsonRpcError> {
    Ok(String::new())
}

use crate::data_types::JsonRpcRequestObject;
use crate::rpc_util::get_error;
use async_std::sync::Arc;
use jsonrpc_v2::Server as JsonRpcServer;
use tide::http::{format_err, Method};
use tide::{Middleware, Next, Request, Response, Result};

pub struct RpcHttpServer<State> {
    rpc_server: Arc<JsonRpcServer<State>>,
}

impl<State> RpcHttpServer<State> {
    pub fn new(rpc_server: JsonRpcServer<State>) -> Self {
        Self {
            rpc_server: Arc::new(rpc_server),
        }
    }
}

#[tide::utils::async_trait]
impl<State: Clone + Send + Sync + 'static> Middleware<State> for RpcHttpServer<State> {
    async fn handle(&self, http_request: Request<State>, next: Next<'_, State>) -> Result {
        if http_request.method() != Method::Post {
            return Err(format_err!("HTTP JSON RPC calls must use POST HTTP method"));
        } else if let Some(content_type) = http_request.content_type() {
            match content_type.essence() {
                "application/json-rpc" => {}
                "application/json" => {}
                "application/jsonrequest" => {}
                _ => {
                    return Err(format_err!(
                        "HTTP JSON RPC calls must provide an appropriate Content-Type header"
                    ));
                }
            }
        }

        let rpc_server = Arc::clone(&self.rpc_server);
        let call: JsonRpcRequestObject = http_request.body_json().await?;
        let rpc_response = rpc_server.handle(call).await;
        let mut http_response: Response = next.run(http_request).await;

        match rpc_response {
            Ok(body) => {
                http_response.set_body(body);
            }
            Err(e) => {
                http_response.set_body(get_error(3, e.message()));
            }
        }

        Ok(http_response)
    }
}

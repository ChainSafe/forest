use crate::rpc_util::get_error;
use async_std::sync::Arc;
use jsonrpc_v2::Server as JsonRpcServer;
use tide::{http::Method, Middleware, Next, Request, Response, Result};

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
            return Ok(res);
        }

        let rpc_response = self.rpc_server.handle(call).await;
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

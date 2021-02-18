use crate::data_types::State;
use crate::rpc_util::get_error;
use async_std::sync::Arc;
use rpc_types::JsonRpcRequestObject;
use tide::http::{format_err, Method};
use tide::{Middleware, Next, Request, Response, Result};

pub async fn rpc_http_handler<DB, KS, B>(
    http_request: Request<State<DB, KS, B>>,
    next: Next<'_, State<DB, KS, B>>,
) -> Result {
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

    let call: JsonRpcRequestObject = http_request.body_json().await?;
    let rpc_server = http_request.state();
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

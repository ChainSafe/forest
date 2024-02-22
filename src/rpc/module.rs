// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::subscription::{
    create_notif_message, PendingSubscriptionSink, Subscribers, SubscriptionKey,
    CANCEL_METHOD_NAME, NOTIF_METHOD_NAME,
};

use jsonrpsee::server::{
    IntoSubscriptionCloseResponse, MethodCallback, MethodResponse, Methods, RegisterMethodError,
};
use jsonrpsee::types::{error::ErrorCode, Id, Params, ResponsePayload, SubscriptionId};

use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(Debug, Clone)]
pub struct RpcModule {
    methods: Methods,
}

impl From<RpcModule> for Methods {
    fn from(module: RpcModule) -> Methods {
        module.methods
    }
}

impl RpcModule {
    /// Create a new module with a given shared `Context`.
    pub fn new() -> Self {
        let mut methods = Methods::default();

        methods
            .verify_and_insert(
                CANCEL_METHOD_NAME,
                MethodCallback::Sync(Arc::new(|id, _params, max_response| {
                    MethodResponse::response(id, ResponsePayload::result(false), max_response)
                })),
            )
            .expect("Inserting a method into an empty methods map is infallible.");

        Self { methods }
    }

    pub fn register_subscription<R, F>(
        &mut self,
        subscribe_method_name: &'static str,
        callback: F,
    ) -> Result<&mut MethodCallback, RegisterMethodError>
    where
        F: (Fn(Params) -> tokio::sync::broadcast::Receiver<R>) + Send + Sync + 'static,
        R: serde::Serialize + Clone + Send + 'static,
    {
        self.register_subscription_raw(subscribe_method_name, {
            move |params, pending| {
                let mut receiver = callback(params);
                tokio::spawn(async move {
                    let sink = pending.accept().await.unwrap();

                    loop {
                        tokio::select! {
                            Ok(msg) = receiver.recv() => {
                                if let Ok(msg) = create_notif_message(&sink, &msg) {
                                    // This fails only if the connection is closed
                                    if let Ok(()) = sink.send(msg).await {
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            },
                            _ = sink.closed() => {
                                break;
                            }
                        }
                    }
                });
            }
        })
    }

    pub fn register_subscription_raw<R, F>(
        &mut self,
        subscribe_method_name: &'static str,
        callback: F,
    ) -> Result<&mut MethodCallback, RegisterMethodError>
    where
        F: (Fn(Params, PendingSubscriptionSink) -> R) + Send + Sync + 'static,
        R: IntoSubscriptionCloseResponse,
    {
        let subscribers = self.verify_and_register_unsubscribe(subscribe_method_name)?;

        // Subscribe
        let callback = {
            self.methods.verify_and_insert(
                subscribe_method_name,
                MethodCallback::Subscription(Arc::new(move |id, params, method_sink, conn| {
                    let sub_id: SubscriptionId<'_> = match id {
                        Id::Null => {
                            return Box::pin(std::future::ready(MethodResponse::error(
                                id,
                                ErrorCode::InvalidParams,
                            )))
                        }
                        Id::Str(ref s) => s.to_string().into(),
                        Id::Number(n) => n.into(),
                    };

                    let uniq_sub = SubscriptionKey {
                        conn_id: conn.conn_id,
                        sub_id: sub_id.clone(),
                    };

                    // response to the subscription call.
                    let (tx, rx) = oneshot::channel();

                    let sink = PendingSubscriptionSink {
                        inner: method_sink.clone(),
                        method: NOTIF_METHOD_NAME,
                        subscribers: subscribers.clone(),
                        uniq_sub,
                        id: id.clone().into_owned(),
                        subscribe: tx,
                        permit: conn.subscription_permit,
                        channel_id: conn.id_provider.next_id(),
                    };

                    callback(params, sink);

                    let id = id.clone().into_owned();

                    Box::pin(async move {
                        match rx.await {
                            Ok(rp) => rp,
                            Err(_) => MethodResponse::error(id, ErrorCode::InternalError),
                        }
                    })
                })),
            )?
        };

        Ok(callback)
    }

    fn verify_and_register_unsubscribe(
        &mut self,
        subscribe_method_name: &'static str,
    ) -> Result<Subscribers, RegisterMethodError> {
        if subscribe_method_name == CANCEL_METHOD_NAME {
            return Err(RegisterMethodError::SubscriptionNameConflict(
                subscribe_method_name.into(),
            ));
        }

        self.methods.verify_method_name(subscribe_method_name)?;

        let subscribers = Subscribers::default();

        Ok(subscribers)
    }
}

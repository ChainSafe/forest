// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::subscription::{
    close_channel_response, PendingSubscriptionSink, Subscribers, SubscriptionKey,
};

use jsonrpsee::server::{
    IntoSubscriptionCloseResponse, MethodCallback, MethodResponse, Methods, RegisterMethodError,
};
use jsonrpsee::types::{error::ErrorCode, Id, Params, ResponsePayload, SubscriptionId};

use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(Debug, Clone)]
pub struct RpcModule<Context> {
    ctx: Arc<Context>,
    methods: Methods,
}

impl<Context> From<RpcModule<Context>> for Methods {
    fn from(module: RpcModule<Context>) -> Methods {
        module.methods
    }
}

impl<Context> RpcModule<Context> {
    /// Create a new module with a given shared `Context`.
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx: Arc::new(ctx),
            methods: Default::default(),
        }
    }

    pub fn register_subscription_raw<R, F>(
        &mut self,
        subscribe_method_name: &'static str,
        notif_method_name: &'static str,
        unsubscribe_method_name: &'static str,
        callback: F,
    ) -> Result<&mut MethodCallback, RegisterMethodError>
    where
        Context: Send + Sync + 'static,
        F: (Fn(Params, PendingSubscriptionSink, Arc<Context>) -> R) + Send + Sync + Clone + 'static,
        R: IntoSubscriptionCloseResponse,
    {
        let subscribers =
            self.verify_and_register_unsubscribe(subscribe_method_name, unsubscribe_method_name)?;
        let ctx = self.ctx.clone();

        // Subscribe
        let callback = {
            self.methods.verify_and_insert(
                subscribe_method_name,
                MethodCallback::Subscription(Arc::new(move |id, params, method_sink, conn| {
                    let sub_id: SubscriptionId<'_> = match id {
                        Id::Null => unreachable!(), // TODO: properly raise an error!
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
                        method: notif_method_name,
                        subscribers: subscribers.clone(),
                        uniq_sub,
                        id: id.clone().into_owned(),
                        subscribe: tx,
                        permit: conn.subscription_permit,
                        channel_id: conn.id_provider.next_id(),
                    };

                    callback(params, sink, ctx.clone());

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
        unsubscribe_method_name: &'static str,
    ) -> Result<Subscribers, RegisterMethodError> {
        if subscribe_method_name == unsubscribe_method_name {
            return Err(RegisterMethodError::SubscriptionNameConflict(
                subscribe_method_name.into(),
            ));
        }

        self.methods.verify_method_name(subscribe_method_name)?;
        self.methods.verify_method_name(unsubscribe_method_name)?;

        let subscribers = Subscribers::default();

        // Unsubscribe
        {
            let subscribers = subscribers.clone();
            self.methods.mut_callbacks().insert(
                unsubscribe_method_name,
                MethodCallback::Unsubscription(Arc::new(
                    move |id, params, conn_id, max_response_size| {
                        let sub_id = match params.one::<SubscriptionId>() {
                            Ok(sub_id) => sub_id,
                            Err(_) => {
                                return MethodResponse::response(
                                    id,
                                    ResponsePayload::result(false),
                                    max_response_size,
                                );
                            }
                        };

                        let key = SubscriptionKey {
                            conn_id,
                            sub_id: sub_id.into_owned(),
                        };
                        let option = subscribers.lock().remove(&key);

                        if let Some((_, _, channel_id)) = option {
                            close_channel_response(channel_id)
                        } else {
                            MethodResponse::error(id, ErrorCode::InternalError)
                        }
                    },
                )),
            );
        }

        Ok(subscribers)
    }
}

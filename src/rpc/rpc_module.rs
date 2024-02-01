// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::subscription::{
    close_channel_response, ForestPendingSubscriptionSink, Subscribers, SubscriptionKey,
};

use jsonrpsee::core::server::MethodResponse;
use jsonrpsee::server::{
    IntoSubscriptionCloseResponse, MethodCallback, Methods, RegisterMethodError,
};
use jsonrpsee::types::error::ErrorCode;
use jsonrpsee::types::{Id, Params, ResponsePayload, SubscriptionId};

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
        fil_pubsub: bool,
        callback: F,
    ) -> Result<&mut MethodCallback, RegisterMethodError>
    where
        Context: Send + Sync + 'static,
        F: (Fn(Params, ForestPendingSubscriptionSink, Arc<Context>) -> R)
            + Send
            + Sync
            + Clone
            + 'static,
        R: IntoSubscriptionCloseResponse,
    {
        let subscribers = self.verify_and_register_unsubscribe(
            subscribe_method_name,
            unsubscribe_method_name,
            fil_pubsub,
        )?;
        let ctx = self.ctx.clone();

        // Subscribe
        let callback = {
            self.methods.verify_and_insert(
                subscribe_method_name,
                MethodCallback::Subscription(Arc::new(move |id, params, method_sink, conn| {
                    //tracing::trace!(target: LOG_TARGET, "Subscribing to {subscribe_method_name}");

                    //tracing::trace!(target: LOG_TARGET, "id: {:?}", &id);

                    let uniq_sub = if fil_pubsub {
                        let sub_id: SubscriptionId<'_> = match id {
                            Id::Null => unreachable!(), // TODO: properly raise an error!
                            Id::Str(ref s) => s.to_string().into(),
                            Id::Number(n) => n.into(),
                        };

                        let uniq_sub = SubscriptionKey {
                            conn_id: conn.conn_id,
                            sub_id: sub_id.clone(),
                        };

                        //tracing::trace!(target: LOG_TARGET, "key: {:?}", &uniq_sub);

                        uniq_sub
                    } else {
                        SubscriptionKey {
                            conn_id: conn.conn_id,
                            sub_id: conn.id_provider.next_id(),
                        }
                    };

                    // response to the subscription call.
                    let (tx, rx) = oneshot::channel();

                    let sink = ForestPendingSubscriptionSink {
                        inner: method_sink.clone(),
                        method: notif_method_name,
                        subscribers: subscribers.clone(),
                        uniq_sub,
                        id: id.clone().into_owned(),
                        subscribe: tx,
                        permit: conn.subscription_permit,
                        channel_id: if fil_pubsub {
                            Some(conn.id_provider.next_id())
                        } else {
                            None
                        },
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
        fil_pubsub: bool,
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
                        //tracing::trace!(target: LOG_TARGET, "Unsubscribing to {subscribe_method_name}");

                        let sub_id = match params.one::<SubscriptionId>() {
                            Ok(sub_id) => sub_id,
                            Err(_) => {
                                // tracing::warn!(
                                // 	target: LOG_TARGET,
                                // 	"Unsubscribe call `{}` failed: couldn't parse subscription id={:?} request id={:?}",
                                // 	unsubscribe_method_name,
                                // 	params,
                                // 	id
                                // );

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
                        let result = option.is_some();

                        if !result {
                            // tracing::debug!(
                            // 	target: LOG_TARGET,
                            // 	"Unsubscribe call `{}` subscription key={:?} not an active subscription",
                            // 	unsubscribe_method_name,
                            // 	key,
                            // );
                        }

                        if fil_pubsub {
                            let channel_id_opt = if let Some((_, _, id)) = option {
                                id
                            } else {
                                None
                            };
                            if let Some(channel_id) = channel_id_opt {
                                close_channel_response(channel_id)
                            } else {
                                MethodResponse::error(id, ErrorCode::InternalError)
                            }
                        } else {
                            MethodResponse::response(
                                id,
                                ResponsePayload::result(result),
                                max_response_size,
                            )
                        }
                    },
                )),
            );
        }

        Ok(subscribers)
    }
}

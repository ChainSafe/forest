pub mod jsonrpc_types;
pub mod openrpc_types;

mod parser;
mod util;

use std::{
    future::{ready, Future, Ready},
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

use futures::future::Either;
use itertools::Itertools as _;
use openrpc_types::{ContentDescriptor, Method, ParamStructure};
use parser::Parser;
use pin_project_lite::pin_project;
use schemars::{
    gen::{SchemaGenerator, SchemaSettings},
    schema::Schema,
    JsonSchema,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::util::Optional;

pub struct SelfDescribingModule<Ctx> {
    inner: jsonrpsee::server::RpcModule<Ctx>,
    schema_generator: SchemaGenerator,
    calling_convention: ParamStructure,
    methods: Vec<Method>,
}

impl<Ctx> SelfDescribingModule<Ctx> {
    pub fn new(ctx: Ctx, calling_convention: ParamStructure) -> Self {
        Self {
            inner: jsonrpsee::server::RpcModule::new(ctx),
            schema_generator: SchemaGenerator::new(SchemaSettings::openapi3()),
            calling_convention,
            methods: vec![],
        }
    }
    pub fn serve<const ARITY: usize, F, Args, R>(
        &mut self,
        method_name: &'static str,
        param_names: [&'static str; ARITY],
        f: F,
    ) -> &mut Self
    where
        F: Wrap<ARITY, Ctx, Args, R>,
        Ctx: Send + Sync + 'static,
        Args: GenerateSchemas,
        R: JsonSchema + for<'de> Deserialize<'de>,
    {
        self.inner
            .register_async_method(method_name, f.wrap(param_names, self.calling_convention))
            .unwrap();
        self.methods.push(Method {
            name: String::from(method_name),
            params: openrpc_types::Params::new(
                Args::generate_schemas(&mut self.schema_generator)
                    .into_iter()
                    .zip_eq(param_names)
                    .map(|((schema, optional), name)| ContentDescriptor {
                        name: String::from(name),
                        schema,
                        required: !optional,
                    }),
            )
            .unwrap(),
            param_structure: self.calling_convention,
            result: Some(ContentDescriptor {
                name: format!("{}-result", method_name),
                schema: R::json_schema(&mut self.schema_generator),
                required: !R::optional(),
            }),
        });
        self
    }
    pub fn finish(self) -> (jsonrpsee::server::RpcModule<Ctx>, openrpc_types::OpenRPC) {
        let Self {
            inner,
            mut schema_generator,
            methods,
            calling_convention: _,
        } = self;
        (
            inner,
            openrpc_types::OpenRPC {
                methods: openrpc_types::Methods::new(methods).unwrap(),
                components: openrpc_types::Components {
                    schemas: schema_generator.take_definitions().into_iter().collect(),
                },
            },
        )
    }
}

/// Wrap a bare function with our argument parsing logic.
/// Turns any `fn foo(ctx, arg0...)` into a function that can be passed to [`jsonrpsee::server::RpcModule::register_async_method`]
pub trait Wrap<const ARITY: usize, Ctx, Args, R> {
    type Future: Future<Output = Result<serde_json::Value, jsonrpsee::types::ErrorObjectOwned>>
        + Send;
    fn wrap(
        self,
        param_names: [&'static str; ARITY],
        calling_convention: ParamStructure,
    ) -> impl Clone
           + Send
           + Sync
           + 'static
           + Fn(jsonrpsee::types::Params<'static>, Arc<Ctx>) -> Self::Future;
}

/// Return all schemas from function arguments.
pub trait GenerateSchemas {
    fn generate_schemas(gen: &mut SchemaGenerator) -> Vec<(Schema, bool)>;
}

macro_rules! do_impls {
    ($arity:literal $(, $arg:ident)* $(,)?) => {
        const _: () = {
            let _assert: [&str; $arity] = [$(stringify!($arg)),*];
        };

        impl<F, $($arg,)* Ctx, Fut, R> Wrap<$arity, Ctx, ($($arg,)*), R> for F
        where
            F: Clone + Send + Sync + 'static + Fn(Arc<Ctx>, $($arg),*) -> Fut,
            Ctx: Send + Sync + 'static,
            $(
                $arg: for<'de> Deserialize<'de> + Clone,
            )*
            Fut: Future<Output = Result<R, jsonrpsee::types::ErrorObjectOwned>> + Send + Sync + 'static,
            R: Serialize,
        {
            type Future = Either<
                Ready<Result<Value, jsonrpsee::types::ErrorObjectOwned>>,
                AndThenDeserializeResponse<Fut>,
            >;

            fn wrap(
                self,
                param_names: [&'static str; $arity],
                calling_convention: ParamStructure,
            ) -> impl Clone
                   + Send
                   + Sync
                   + 'static
                   + Fn(jsonrpsee::types::Params<'static>, Arc<Ctx>) -> Self::Future {
                move |params, ctx| {
                    let params = match params.as_str().map(serde_json::from_str).transpose() {
                        Ok(it) => it,
                        Err(e) => {
                            return Either::Left(ready(Err(error2error(
                                jsonrpc_types::Error::invalid_params(e, None),
                            ))))
                        }
                    };
                    #[allow(unused_variables, unused_mut)]
                    let mut parser = match Parser::new(params, &param_names, calling_convention) {
                        Ok(it) => it,
                        Err(e) => return Either::Left(ready(Err(error2error(e)))),
                    };

                    Either::Right(AndThenDeserializeResponse {
                        inner: self(
                            ctx,
                            $(
                                match parser.parse::<$arg>() {
                                    Ok(it) => it,
                                    Err(e) => return Either::Left(ready(Err(error2error(e)))),
                                },
                            )*
                        ),
                    })
                }
            }
        }

        #[automatically_derived]
        impl<$($arg,)*> GenerateSchemas for ($($arg,)*)
        where
            $($arg: JsonSchema + for<'de> Deserialize<'de>,)*
        {
            fn generate_schemas(gen: &mut SchemaGenerator) -> Vec<(Schema, bool)> {
                vec![
                    $(($arg::json_schema(gen), $arg::optional())),*
                ]
            }
        }

    };
}

do_impls!(0);
do_impls!(1, T0);
do_impls!(2, T0, T1);
do_impls!(3, T0, T1, T2);
do_impls!(4, T0, T1, T2, T3);
do_impls!(5, T0, T1, T2, T3, T4);
do_impls!(6, T0, T1, T2, T3, T4, T5);
do_impls!(7, T0, T1, T2, T3, T4, T5, T6);

pin_project! {
    pub struct AndThenDeserializeResponse<F> {
        #[pin]
        inner: F
    }
}

impl<R, F> Future for AndThenDeserializeResponse<F>
where
    F: Future<Output = Result<R, jsonrpsee::types::ErrorObjectOwned>>,
    R: Serialize,
{
    type Output = Result<Value, jsonrpsee::types::ErrorObjectOwned>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(
            serde_json::to_value(ready!(self.project().inner.poll(cx))?)
                .map_err(|e| {
                    jsonrpc_types::Error::internal_error(
                        "error deserializing return value for handler",
                        json!({
                            "type": std::any::type_name::<R>(),
                            "error": e.to_string()
                        }),
                    )
                })
                .map_err(error2error),
        )
    }
}

fn error2error(ours: jsonrpc_types::Error) -> jsonrpsee::types::ErrorObjectOwned {
    let jsonrpc_types::Error {
        code,
        message,
        data,
    } = ours;
    jsonrpsee::types::ErrorObject::owned(code as i32, message, data)
}

// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Forest wishes to provide [OpenRPC](http://open-rpc.org) definitions for
//! Filecoin APIs.
//! To do this, it needs:
//! - [JSON Schema](https://json-schema.org/) definitions for all the argument
//!   and return types.
//! - The number of arguments ([arity](https://en.wikipedia.org/wiki/Arity)) and
//!   names of those arguments for each RPC method.
//!
//! As a secondary objective, we wish to provide an RPC client for our CLI, and
//! internal tests against Lotus.
//!
//! The [`RpcMethod`] trait encapsulates all the above at a single site.
//! - [`schemars::JsonSchema`] provides schema definitions,
//! - [`RpcMethod`] defining arity and actually dispatching the function calls.
//!
//! [`SelfDescribingRpcModule`] actually does the work to create the OpenRPC document.
//!
//! # Implementation guidelines.
//! - [`RpcMethod`] MUST only be implemented on _uninhabited types_, i.e empty
//!   `enum`s.
//! - [`RpcMethod::Params`] MUST always be a tuple.
//!   - Do not accept `LotusJson<(T0, T1)>`.
//!     This is misuse of the `LotusJson` API.
//! - Params SHOULD be refactored away from using `#[serde(with = "crate::lotus_json")]`
//!   to allow for [`JsonSchema`] to be `#[derive(..)]`-d appropriately.
//! - You SHOULD implement the [`RpcMethod::handle`] as an `async fn`, returning
//!   `Result<Self::Ok, JsonRpcError>`.
//! - You SHOULD relax the bounds on [`Ctx`] as appropriate.
//!   E.g from `Ctx<impl Blockstore + Send>` to `Ctx<impl Blockstore>`.
//! - You SHOULD destructure params in [`RpcMethod::handle`].
//! - Method-specific structs SHOULD live in the same file as the [`RpcMethod`]
//!   implementation.
//!
//! # Limitations
//! - We don't provide any discrimination of error types.

pub mod jsonrpc_types;
pub mod openrpc_types;

mod parser;
mod util;

use crate::rpc_api::data_types::RPCState;

use self::{jsonrpc_types::RequestParameters, util::Optional as _};
use super::error::JsonRpcError as Error;

use futures::future::Either;
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::{MethodsError, RpcModule};
use openrpc_types::{ContentDescriptor, Method, ParamListError, ParamStructure};
use parser::Parser;
use schemars::{
    gen::{SchemaGenerator, SchemaSettings},
    schema::Schema,
    JsonSchema,
};
use serde::Serialize;
use serde::{
    de::{DeserializeOwned, Error as _, Unexpected},
    Deserialize,
};
use std::{future::Future, sync::Arc};

/// Type to be used by [`RpcMethod::handle`].
// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4007
//                  avoid double indirection
pub type Ctx<T> = Arc<Arc<RPCState<T>>>;
/// Type to be used by [`SelfDescribingRpcModule`] and [`RpcModule`].
pub type ModuleState<T> = Arc<RPCState<T>>;

/// A definition of an RPC method handler which can be registered with a
/// [`SelfDescribingRpcModule`].
///
/// Note, an earlier draft of this trait had an additional type parameter for `Ctx`
/// for generality.
/// However, fixing it as [`Ctx<...>`] saves on complexity/confusion for implementors,
/// at the expense of handler flexibility, which could come back to bite us.
/// - All handlers accept [`RPCState`].
/// - All `Ctx`s must be `Send + Sync + 'static` due to bounds on [`RpcModule`].
/// - Handlers don't specialize on top of the given bounds, but they MAY relax them.
pub trait RpcMethod<const ARITY: usize> {
    /// Method name.
    const NAME: &'static str;
    /// Name of each argument, MUST be unique.
    const PARAM_NAMES: [&'static str; ARITY];
    /// Types of each argument. [`Option`]-al arguments MUST follow mandatory ones.
    type Params: Params<ARITY>;
    /// Return value of this method.
    type Ok;
    /// Logic for this method.
    fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        params: Self::Params,
    ) -> impl Future<Output = Result<Self::Ok, Error>> + Send;
}

/// Utility methods, defined as an extension trait to avoid having to specify
/// `ARITY` in user code.
pub trait RpcMethodExt<const ARITY: usize>: RpcMethod<ARITY> {
    /// Convert from typed handler parameters to un-typed JSON-RPC parameters.
    ///
    /// Exposes errors from [`Params::unparse`]
    fn build_params(
        params: Self::Params,
        calling_convention: ConcreteCallingConvention,
    ) -> Result<RequestParameters, serde_json::Error>
    where
        Self::Params: Serialize,
    {
        let args = params.unparse()?;
        match calling_convention {
            ConcreteCallingConvention::ByPosition => {
                Ok(RequestParameters::ByPosition(Vec::from(args)))
            }
            ConcreteCallingConvention::ByName => Ok(RequestParameters::ByName(
                itertools::zip_eq(Self::PARAM_NAMES.into_iter().map(String::from), args).collect(),
            )),
        }
    }
    /// Generate a full `OpenRPC` method definition for this endpoint.
    fn openrpc<'de>(
        gen: &mut SchemaGenerator,
        calling_convention: ParamStructure,
    ) -> Result<Method, ParamListError>
    where
        Self::Ok: JsonSchema + Deserialize<'de>,
    {
        Ok(Method {
            name: String::from(Self::NAME),
            params: openrpc_types::Params::new(
                itertools::zip_eq(Self::PARAM_NAMES, Self::Params::schemas(gen)).map(
                    |(name, (schema, optional))| ContentDescriptor {
                        name: String::from(name),
                        schema,
                        required: !optional,
                    },
                ),
            )?,
            param_structure: calling_convention,
            result: Some(ContentDescriptor {
                name: format!("{}::Result", Self::NAME),
                schema: Self::Ok::json_schema(gen),
                required: !Self::Ok::optional(),
            }),
        })
    }
    /// Register this method with an [`RpcModule`].
    fn register_raw(
        module: &mut RpcModule<ModuleState<impl Blockstore + Send + Sync + 'static>>,
        calling_convention: ParamStructure,
    ) -> Result<&mut jsonrpsee::MethodCallback, jsonrpsee::core::RegisterMethodError>
    where
        Self::Ok: Serialize + Clone + 'static,
    {
        module.register_async_method(Self::NAME, move |params, ctx| async move {
            let raw = params
                .as_str()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|e| Error::invalid_params(e, None))?;
            let params = Self::Params::parse(raw, Self::PARAM_NAMES, calling_convention)?;
            let ok = Self::handle(ctx, params).await?;
            Result::<_, jsonrpsee::types::ErrorObjectOwned>::Ok(ok)
        })
    }
    /// Register this method and generate a schema entry for it in a [`SelfDescribingRpcModule`].
    fn register<'de>(
        module: &mut SelfDescribingRpcModule<ModuleState<impl Blockstore + Send + Sync + 'static>>,
    ) where
        Self::Ok: Serialize + Clone + 'static,
        Self::Ok: JsonSchema + Deserialize<'de>,
    {
        Self::register_raw(&mut module.inner, module.calling_convention).unwrap();
        module
            .methods
            .push(Self::openrpc(&mut module.schema_generator, module.calling_convention).unwrap());
    }
    /// Call this method on an [`RpcModule`].
    fn call(
        module: &RpcModule<Ctx<impl Blockstore + Send + Sync + 'static>>,
        params: Self::Params,
        calling_convention: ConcreteCallingConvention,
    ) -> impl Future<Output = Result<Self::Ok, MethodsError>> + Send
    where
        Self::Params: Serialize,
        Self::Ok: DeserializeOwned + Clone + Send,
    {
        macro_rules! tri {
            ($expr:expr) => {
                match $expr {
                    Ok(it) => it,
                    Err(e) => return Either::Left(futures::future::ready(Err(e.into()))),
                }
            };
        }
        match tri!(Self::build_params(params, calling_convention)) {
            RequestParameters::ByPosition(args) => {
                let mut builder = jsonrpsee::core::params::ArrayParams::new();
                for arg in args {
                    tri!(builder.insert(arg))
                }
                Either::Right(Either::Left(module.call(Self::NAME, builder)))
            }
            RequestParameters::ByName(args) => {
                let mut builder = jsonrpsee::core::params::ObjectParams::new();
                for (name, value) in args {
                    tri!(builder.insert(&name, value));
                }
                Either::Right(Either::Right(module.call(Self::NAME, builder)))
            }
        }
    }
}
impl<const ARITY: usize, T> RpcMethodExt<ARITY> for T where T: RpcMethod<ARITY> {}

/// A tuple of `ARITY` arguments.
///
/// This should NOT be manually implemented.
pub trait Params<const ARITY: usize> {
    /// A [`Schema`] and [`Optional::optional`](`util::Optional::optional`)
    /// pair for argument, in-order.
    fn schemas(gen: &mut SchemaGenerator) -> [(Schema, bool); ARITY];
    /// Convert from raw request parameters, to the argument tuple required by
    /// [`RpcMethod::handle`]
    fn parse(
        raw: Option<RequestParameters>,
        names: [&str; ARITY],
        calling_convention: ParamStructure,
    ) -> Result<Self, Error>
    where
        Self: Sized;
    /// Convert from an argument tuple to un-typed JSON.
    ///
    /// Exposes de-serialization errors, or mis-implementation of this trait.
    fn unparse(&self) -> Result<[serde_json::Value; ARITY], serde_json::Error>
    where
        Self: Serialize,
    {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Array(args)) => match args.try_into() {
                Ok(it) => Ok(it),
                Err(_) => Err(serde_json::Error::custom("ARITY mismatch")),
            },
            Ok(it) => Err(serde_json::Error::invalid_type(
                unexpected(&it),
                &"a Vec with an item for each argument",
            )),
            Err(e) => Err(e),
        }
    }
}

// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4066
#[allow(unused)]
fn unexpected(v: &serde_json::Value) -> Unexpected<'_> {
    match v {
        serde_json::Value::Null => Unexpected::Unit,
        serde_json::Value::Bool(it) => Unexpected::Bool(*it),
        serde_json::Value::Number(it) => match (it.as_f64(), it.as_i64(), it.as_u64()) {
            (None, None, None) => Unexpected::Other("Number"),
            (Some(it), _, _) => Unexpected::Float(it),
            (_, Some(it), _) => Unexpected::Signed(it),
            (_, _, Some(it)) => Unexpected::Unsigned(it),
        },
        serde_json::Value::String(it) => Unexpected::Str(it),
        serde_json::Value::Array(_) => Unexpected::Seq,
        serde_json::Value::Object(_) => Unexpected::Map,
    }
}

macro_rules! do_impls {
    ($arity:literal $(, $arg:ident)* $(,)?) => {
        const _: () = {
            let _assert: [&str; $arity] = [$(stringify!($arg)),*];
        };

        impl<$($arg),*> Params<$arity> for ($($arg,)*)
        where
            $($arg: DeserializeOwned + Serialize + JsonSchema),*
        {
            fn parse(
                raw: Option<RequestParameters>,
                arg_names: [&str; $arity],
                calling_convention: ParamStructure,
            ) -> Result<Self, Error> {
                let mut _parser = Parser::new(raw, &arg_names, calling_convention)?;
                Ok(($(_parser.parse::<$arg>()?,)*))
            }
            fn schemas(_gen: &mut SchemaGenerator) -> [(Schema, bool); $arity] {
                [$(($arg::json_schema(_gen), $arg::optional())),*]
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
do_impls!(8, T0, T1, T2, T3, T4, T5, T6, T7);
do_impls!(9, T0, T1, T2, T3, T4, T5, T6, T7, T8);
do_impls!(10, T0, T1, T2, T3, T4, T5, T6, T7, T8, T9);

pub struct SelfDescribingRpcModule<Ctx> {
    inner: jsonrpsee::server::RpcModule<Ctx>,
    schema_generator: SchemaGenerator,
    calling_convention: ParamStructure,
    methods: Vec<Method>,
}

impl<Ctx> SelfDescribingRpcModule<Ctx> {
    pub fn new(ctx: Ctx, calling_convention: ParamStructure) -> Self {
        Self {
            inner: jsonrpsee::server::RpcModule::new(ctx),
            schema_generator: SchemaGenerator::new(SchemaSettings::openapi3()),
            calling_convention,
            methods: vec![],
        }
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

// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4066
#[allow(unused)]
/// [`openrpc_types::ParamStructure`] describes accepted param format.
/// This is an actual param format, used to decide how to construct arguments.
pub enum ConcreteCallingConvention {
    ByPosition,
    ByName,
}

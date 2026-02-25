// Copyright 2019-2026 ChainSafe Systems
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

pub mod jsonrpc_types;

mod parser;
mod util;

use crate::lotus_json::HasLotusJson;

use self::{jsonrpc_types::RequestParameters, util::Optional as _};
use super::error::ServerError as Error;
use ahash::HashMap;
use anyhow::Context as _;
use enumflags2::{BitFlags, bitflags, make_bitflags};
use fvm_ipld_blockstore::Blockstore;
use http::{Extensions, Uri};
use jsonrpsee::RpcModule;
use openrpc_types::{ContentDescriptor, Method, ParamStructure, ReferenceOr};
use parser::Parser;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{
    Deserialize, Serialize,
    de::{Error as _, Unexpected},
};
use std::{future::Future, str::FromStr, sync::Arc};
use strum::EnumString;

/// Type to be used by [`RpcMethod::handle`].
pub type Ctx<T> = Arc<crate::rpc::RPCState<T>>;

/// A definition of an RPC method handler which:
/// - can be [registered](RpcMethodExt::register) with an [`RpcModule`].
/// - can describe itself in OpenRPC.
///
/// Note, an earlier draft of this trait had an additional type parameter for `Ctx`
/// for generality.
/// However, fixing it as [`Ctx<...>`] saves on complexity/confusion for implementors,
/// at the expense of handler flexibility, which could come back to bite us.
/// - All handlers accept the same type.
/// - All `Ctx`s must be `Send + Sync + 'static` due to bounds on [`RpcModule`].
/// - Handlers don't specialize on top of the given bounds, but they MAY relax them.
pub trait RpcMethod<const ARITY: usize> {
    /// Number of required parameters, defaults to `ARITY`.
    const N_REQUIRED_PARAMS: usize = ARITY;
    /// Method name.
    const NAME: &'static str;
    /// Alias for `NAME`. Note that currently this is not reflected in the OpenRPC spec.
    const NAME_ALIAS: Option<&'static str> = None;
    /// Name of each argument, MUST be unique.
    const PARAM_NAMES: [&'static str; ARITY];
    /// See [`ApiPaths`].
    const API_PATHS: BitFlags<ApiPaths>;
    /// See [`Permission`]
    const PERMISSION: Permission;
    /// Becomes [`openrpc_types::Method::summary`].
    const SUMMARY: Option<&'static str> = None;
    /// Becomes [`openrpc_types::Method::description`].
    const DESCRIPTION: Option<&'static str> = None;
    /// Types of each argument. [`Option`]-al arguments MUST follow mandatory ones.
    type Params: Params<ARITY>;
    /// Return value of this method.
    type Ok: HasLotusJson;
    /// Logic for this method.
    fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        params: Self::Params,
        ext: &Extensions,
    ) -> impl Future<Output = Result<Self::Ok, Error>> + Send;
    /// If it a subscription method. Defaults to false.
    const SUBSCRIPTION: bool = false;
}

/// The permission required to call an RPC method.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::Display,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// admin
    Admin,
    /// sign
    Sign,
    /// write
    Write,
    /// read
    Read,
}

/// Which paths should this method be exposed on?
///
/// This information is important when using [`crate::rpc::client`].
#[bitflags]
#[repr(u8)]
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    clap::ValueEnum,
    EnumString,
    Deserialize,
    Serialize,
)]
pub enum ApiPaths {
    /// Only expose this method on `/rpc/v0`
    #[strum(ascii_case_insensitive)]
    V0 = 0b00000001,
    /// Only expose this method on `/rpc/v1`
    #[strum(ascii_case_insensitive)]
    #[default]
    V1 = 0b00000010,
    /// Only expose this method on `/rpc/v2`
    #[strum(ascii_case_insensitive)]
    V2 = 0b00000100,
}

impl ApiPaths {
    pub const fn all() -> BitFlags<Self> {
        // Not containing V2 until it's released in Lotus.
        make_bitflags!(Self::{ V0 | V1 })
    }

    // Remove this helper once all RPC methods are migrated to V2.
    // We're incrementally migrating methods to V2 support. Once complete,
    // update all() to include V2 and remove this temporary helper.
    pub const fn all_with_v2() -> BitFlags<Self> {
        Self::all().union_c(make_bitflags!(Self::{ V2 }))
    }

    pub fn from_uri(uri: &Uri) -> anyhow::Result<Self> {
        Ok(Self::from_str(uri.path().trim_start_matches("/rpc/"))?)
    }

    pub fn path(&self) -> &'static str {
        match self {
            Self::V0 => "rpc/v0",
            Self::V1 => "rpc/v1",
            Self::V2 => "rpc/v2",
        }
    }
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
    ) -> Result<RequestParameters, serde_json::Error> {
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

    fn parse_params(
        params_raw: Option<impl AsRef<str>>,
        calling_convention: ParamStructure,
    ) -> anyhow::Result<Self::Params> {
        Ok(Self::Params::parse(
            params_raw
                .map(|s| serde_json::from_str(s.as_ref()))
                .transpose()?,
            Self::PARAM_NAMES,
            calling_convention,
            Self::N_REQUIRED_PARAMS,
        )?)
    }

    /// Generate a full `OpenRPC` method definition for this endpoint.
    fn openrpc<'de>(
        g: &mut SchemaGenerator,
        calling_convention: ParamStructure,
        method_name: &'static str,
    ) -> Method
    where
        <Self::Ok as HasLotusJson>::LotusJson: JsonSchema + Deserialize<'de>,
    {
        Method {
            name: String::from(method_name),
            params: itertools::zip_eq(Self::PARAM_NAMES, Self::Params::schemas(g))
                .enumerate()
                .map(|(pos, (name, (schema, nullable)))| {
                    let required = pos <= Self::N_REQUIRED_PARAMS;
                    if !required && !nullable {
                        panic!("Optional parameter at position {pos} should be of an optional type. method={method_name}, param_name={name}");
                    }
                    ReferenceOr::Item(ContentDescriptor {
                        name: String::from(name),
                        schema,
                        required: Some(required),
                        ..Default::default()
                    })
                })
                .collect(),
            param_structure: Some(calling_convention),
            result: Some(ReferenceOr::Item(ContentDescriptor {
                name: format!("{method_name}.Result"),
                schema: g.subschema_for::<<Self::Ok as HasLotusJson>::LotusJson>(),
                required: Some(!<Self::Ok as HasLotusJson>::LotusJson::optional()),
                ..Default::default()
            })),
            summary: Self::SUMMARY.map(Into::into),
            description: Self::DESCRIPTION.map(Into::into),
            ..Default::default()
        }
    }

    /// Register a method with an [`RpcModule`].
    fn register(
        modules: &mut HashMap<
            ApiPaths,
            RpcModule<crate::rpc::RPCState<impl Blockstore + Send + Sync + 'static>>,
        >,
        calling_convention: ParamStructure,
    ) -> Result<(), jsonrpsee::core::RegisterMethodError>
    where
        <Self::Ok as HasLotusJson>::LotusJson: Clone + 'static,
    {
        use clap::ValueEnum as _;

        assert!(
            Self::N_REQUIRED_PARAMS <= ARITY,
            "N_REQUIRED_PARAMS({}) can not be greater than ARITY({ARITY}) in {}",
            Self::N_REQUIRED_PARAMS,
            Self::NAME
        );

        for api_version in ApiPaths::value_variants() {
            if Self::API_PATHS.contains(*api_version)
                && let Some(module) = modules.get_mut(api_version)
            {
                module.register_async_method(
                    Self::NAME,
                    move |params, ctx, extensions| async move {
                        let params = Self::parse_params(params.as_str(), calling_convention)
                            .map_err(|e| Error::invalid_params(e, None))?;
                        let ok = Self::handle(ctx, params, &extensions).await?;
                        Result::<_, jsonrpsee::types::ErrorObjectOwned>::Ok(ok.into_lotus_json())
                    },
                )?;
                if let Some(alias) = Self::NAME_ALIAS {
                    module.register_alias(alias, Self::NAME)?
                }
            }
        }
        Ok(())
    }
    /// Returns [`Err`] if any of the parameters fail to serialize.
    fn request(params: Self::Params) -> Result<crate::rpc::Request<Self::Ok>, serde_json::Error> {
        // hardcode calling convention because lotus is by-position only
        let params = Self::request_params(params)?;
        Ok(crate::rpc::Request {
            method_name: Self::NAME.into(),
            params,
            result_type: std::marker::PhantomData,
            api_paths: Self::API_PATHS,
            timeout: *crate::rpc::DEFAULT_REQUEST_TIMEOUT,
        })
    }

    fn request_params(params: Self::Params) -> Result<serde_json::Value, serde_json::Error> {
        // hardcode calling convention because lotus is by-position only
        Ok(
            match Self::build_params(params, ConcreteCallingConvention::ByPosition)? {
                RequestParameters::ByPosition(mut it) => {
                    // Omit optional parameters when they are null
                    // This can be refactored into using `while pop_if`
                    // when the API is stablized.
                    while Self::N_REQUIRED_PARAMS < it.len() {
                        match it.last() {
                            Some(last) if last.is_null() => it.pop(),
                            _ => break,
                        };
                    }
                    serde_json::Value::Array(it)
                }
                RequestParameters::ByName(it) => serde_json::Value::Object(it),
            },
        )
    }

    /// Creates a request, using the alias method name if `use_alias` is `true`.
    fn request_with_alias(
        params: Self::Params,
        use_alias: bool,
    ) -> anyhow::Result<crate::rpc::Request<Self::Ok>> {
        let params = Self::request_params(params)?;
        let name = if use_alias {
            Self::NAME_ALIAS.context("alias is None")?
        } else {
            Self::NAME
        };

        Ok(crate::rpc::Request {
            method_name: name.into(),
            params,
            result_type: std::marker::PhantomData,
            api_paths: Self::API_PATHS,
            timeout: *crate::rpc::DEFAULT_REQUEST_TIMEOUT,
        })
    }
    fn call_raw(
        client: &crate::rpc::client::Client,
        params: Self::Params,
    ) -> impl Future<Output = Result<<Self::Ok as HasLotusJson>::LotusJson, jsonrpsee::core::ClientError>>
    {
        async {
            // TODO(forest): https://github.com/ChainSafe/forest/issues/4032
            //               Client::call has an inappropriate HasLotusJson
            //               bound, work around it for now.
            let json = client.call(Self::request(params)?.map_ty()).await?;
            Ok(serde_json::from_value(json)?)
        }
    }
    fn call(
        client: &crate::rpc::client::Client,
        params: Self::Params,
    ) -> impl Future<Output = Result<Self::Ok, jsonrpsee::core::ClientError>> {
        async {
            Self::call_raw(client, params)
                .await
                .map(Self::Ok::from_lotus_json)
        }
    }
}
impl<const ARITY: usize, T> RpcMethodExt<ARITY> for T where T: RpcMethod<ARITY> {}

/// A tuple of `ARITY` arguments.
///
/// This should NOT be manually implemented.
pub trait Params<const ARITY: usize>: HasLotusJson {
    /// A [`Schema`] and [`Optional::optional`](`util::Optional::optional`)
    /// schema-nullable pair for argument, in-order.
    fn schemas(g: &mut SchemaGenerator) -> [(Schema, bool); ARITY];
    /// Convert from raw request parameters, to the argument tuple required by
    /// [`RpcMethod::handle`]
    fn parse(
        raw: Option<RequestParameters>,
        names: [&str; ARITY],
        calling_convention: ParamStructure,
        n_required: usize,
    ) -> Result<Self, Error>
    where
        Self: Sized;
    /// Convert from an argument tuple to un-typed JSON.
    ///
    /// Exposes de-serialization errors, or mis-implementation of this trait.
    fn unparse(self) -> Result<[serde_json::Value; ARITY], serde_json::Error> {
        match self.into_lotus_json_value() {
            Ok(serde_json::Value::Array(args)) => match args.try_into() {
                Ok(it) => Ok(it),
                Err(_) => Err(serde_json::Error::custom("ARITY mismatch")),
            },
            Ok(serde_json::Value::Null) if ARITY == 0 => {
                Ok(std::array::from_fn(|_ix| Default::default()))
            }
            Ok(it) => Err(serde_json::Error::invalid_type(
                unexpected(&it),
                &"a Vec with an item for each argument",
            )),
            Err(e) => Err(e),
        }
    }
}

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
            $($arg: HasLotusJson + Clone, <$arg as HasLotusJson>::LotusJson: JsonSchema, )*
        {
            fn parse(
                raw: Option<RequestParameters>,
                arg_names: [&str; $arity],
                calling_convention: ParamStructure,
                n_required: usize,
            ) -> Result<Self, Error> {
                let mut _parser = Parser::new(raw, &arg_names, calling_convention, n_required)?;
                Ok(($(_parser.parse::<crate::lotus_json::LotusJson<$arg>>()?.into_inner(),)*))
            }
            fn schemas(_gen: &mut SchemaGenerator) -> [(Schema, bool); $arity] {
                [$((_gen.subschema_for::<$arg::LotusJson>(), $arg::LotusJson::optional())),*]
            }
        }
    };
}

do_impls!(0);
do_impls!(1, T0);
do_impls!(2, T0, T1);
do_impls!(3, T0, T1, T2);
do_impls!(4, T0, T1, T2, T3);
// do_impls!(5, T0, T1, T2, T3, T4);
// do_impls!(6, T0, T1, T2, T3, T4, T5);
// do_impls!(7, T0, T1, T2, T3, T4, T5, T6);
// do_impls!(8, T0, T1, T2, T3, T4, T5, T6, T7);
// do_impls!(9, T0, T1, T2, T3, T4, T5, T6, T7, T8);
// do_impls!(10, T0, T1, T2, T3, T4, T5, T6, T7, T8, T9);

/// [`openrpc_types::ParamStructure`] describes accepted param format.
/// This is an actual param format, used to decide how to construct arguments.
pub enum ConcreteCallingConvention {
    ByPosition,
    #[allow(unused)] // included for completeness
    ByName,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_paths_from_uri() {
        let v0 = ApiPaths::from_uri(&"http://127.0.0.1:2345/rpc/v0".parse().unwrap()).unwrap();
        assert_eq!(v0, ApiPaths::V0);
        let v1 = ApiPaths::from_uri(&"http://127.0.0.1:2345/rpc/v1".parse().unwrap()).unwrap();
        assert_eq!(v1, ApiPaths::V1);
        let v2 = ApiPaths::from_uri(&"http://127.0.0.1:2345/rpc/v2".parse().unwrap()).unwrap();
        assert_eq!(v2, ApiPaths::V2);

        ApiPaths::from_uri(&"http://127.0.0.1:2345/rpc/v3".parse().unwrap()).unwrap_err();
    }
}

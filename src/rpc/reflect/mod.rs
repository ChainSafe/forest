use jsonrpsee::types::Params;
use schemars::gen::SchemaGenerator;
use std::future::Future;

use crate::rpc_client::JsonRpcError;

use self::openrpc_types::ContentDescriptor;
use self::openrpc_types::ParamStructure;

mod jsonrpc_types;
mod openrpc_types;
mod parser;
mod util;

struct WrappedModule<Ctx> {
    inner: jsonrpsee::server::RpcModule<Ctx>,
    methods: Vec<openrpc_types::Method>,
}

impl<Ctx> WrappedModule<Ctx> {
    pub fn register<const ARITY: usize, F, Args, Fut, T>(&mut self, handler: F) -> &mut Self
    where
        F: Wrap<ARITY, Args, Ctx>,
    {
        // self.inner.register_async_method(
        //     "test",
        // )
        self
    }
}

// Fut: Future<Output = R> + Send,
// Fun: (Fn(Params<'static>, Arc<Context>) -> Fut) + Clone + Send + Sync + 'static,

trait Wrap<const ARITY: usize, Args, Ctx> {
    type Fut: Future<Output = Result<serde_json::Value, JsonRpcError>> + Send;
    fn wrap(
        self,
        param_names: [&str; ARITY],
        return_name: &str,
        calling_convention: ParamStructure,
        gen: &mut SchemaGenerator,
    ) -> (
        impl Fn(Params<'static>, Ctx) -> Self::Fut,
        openrpc_types::Params,
        ParamStructure,
        Option<ContentDescriptor>,
    );
}

const fn assert_impl<T>(_: T)
where
    T: Copy,
{
}
const _: () = assert_impl(crate::rpc::chain_api::chain_get_path2::<crate::db::MemoryDB>);

// impl<Ctx, F, T0, T1, Fut, R> Wrap<2, (T0, T1), Ctx> for F where F: Fn() {}

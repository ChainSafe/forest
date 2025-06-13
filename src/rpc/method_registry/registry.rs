// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::HasLotusJson;
use crate::rpc::actor_registry::{ACTOR_REGISTRY, get_actor_type_from_code};
use crate::shim::machine::BuiltinActor;
use crate::shim::message::MethodNum;
use ahash::{HashMap, HashMapExt};
use anyhow::{Result, anyhow};
use cid::Cid;
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde_json::Value;

// Global registry for method parameter deserialization
static METHOD_REGISTRY: Lazy<MethodRegistry> = Lazy::new(|| {
    let mut registry = MethodRegistry::new();
    register_known_methods(&mut registry);
    registry
});

type ParamDeserializerFn = Box<dyn Fn(&[u8]) -> Result<Value> + Send + Sync>;

pub struct MethodRegistry {
    // (code_cid, method_num) -> method param deserializer
    deserializers: HashMap<(Cid, MethodNum), ParamDeserializerFn>,
}

impl MethodRegistry {
    fn new() -> Self {
        Self {
            deserializers: HashMap::new(),
        }
    }

    pub(crate) fn register_method<P: 'static + DeserializeOwned + HasLotusJson>(
        &mut self,
        code_cid: Cid,
        method_num: MethodNum,
        deserializer: fn(&[u8]) -> Result<P>,
    ) {
        let boxed_deserializer: ParamDeserializerFn = Box::new(move |bytes| -> Result<Value> {
            let param: P = deserializer(bytes)?;
            serde_json::to_value(param.into_lotus_json())
                .map_err(|e| anyhow!("Failed to serialize method param into JSON: {}", e))
        });

        self.deserializers
            .insert((code_cid, method_num), boxed_deserializer);
    }

    fn deserialize_params(
        &self,
        code_cid: &Cid,
        method_num: MethodNum,
        params_bytes: &[u8],
    ) -> Result<Option<Value>> {
        if let Some(deserializer) = self.deserializers.get(&(*code_cid, method_num)) {
            return Ok(Some(deserializer(params_bytes)?));
        }

        let (actor_type, version) = get_actor_type_from_code(code_cid)?;

        Err(anyhow!(
            "No deserializer registered for actor type {:?} (v{}), method {}",
            actor_type,
            version,
            method_num
        ))
    }
}

pub fn deserialize_params(
    code_cid: &Cid,
    method_num: MethodNum,
    params_bytes: &[u8],
) -> Result<Option<Value>> {
    METHOD_REGISTRY.deserialize_params(code_cid, method_num, params_bytes)
}

fn register_known_methods(registry: &mut MethodRegistry) {
    use crate::rpc::method_registry::actors::{account, evm, miner};

    for (&cid, &(actor_type, _version)) in ACTOR_REGISTRY.iter() {
        match actor_type {
            BuiltinActor::Account => account::register_account_actor_methods(registry, cid),
            BuiltinActor::Miner => miner::register_miner_actor_methods(registry, cid),
            BuiltinActor::EVM => evm::register_evm_actor_methods(registry, cid),
            _ => {}
        }
    }
}

macro_rules! register_actor_methods {
    ($registry:expr, $code_cid:expr, [
        $( ($method:expr, $param_type:ty) ),* $(,)?
    ]) => {
        $(
            $registry.register_method(
                $code_cid,
                $method as MethodNum,
                |bytes| -> Result<$param_type> { Ok(fvm_ipld_encoding::from_slice(bytes)?) },
            );
        )*
    };
}

pub(crate) use register_actor_methods;

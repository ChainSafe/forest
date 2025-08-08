// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::HasLotusJson;
use crate::rpc::registry::actors_reg::{ACTOR_REGISTRY, ActorRegistry};
use crate::shim::machine::BuiltinActor;
use crate::shim::message::MethodNum;
use ahash::{HashMap, HashMapExt};
use anyhow::{Context, Result, bail};
use cid::Cid;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::LazyLock;

// Global registry for method parameter deserialization
static METHOD_REGISTRY: LazyLock<MethodRegistry> =
    LazyLock::new(MethodRegistry::with_known_methods);

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

    fn with_known_methods() -> Self {
        let mut registry = Self::new();
        registry.register_known_methods();
        registry
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
                .context("Failed to serialize method param into JSON")
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

        let (actor_type, version) = ActorRegistry::get_actor_details_from_code(code_cid)?;

        bail!(
            "No deserializer registered for actor type {:?} (v{}), method {}",
            actor_type,
            version,
            method_num
        );
    }

    fn register_known_methods(&mut self) {
        use crate::rpc::registry::actors::{account, evm, init, miner, power, reward, system};

        for (&cid, &(actor_type, version)) in ACTOR_REGISTRY.iter() {
            match actor_type {
                BuiltinActor::Account => {
                    account::register_account_actor_methods(self, cid, version)
                }
                BuiltinActor::Miner => miner::register_miner_actor_methods(self, cid, version),
                BuiltinActor::EVM => evm::register_evm_actor_methods(self, cid, version),
                BuiltinActor::Init => init::register_actor_methods(self, cid, version),
                BuiltinActor::System => system::register_actor_methods(self, cid, version),
                BuiltinActor::Power => power::register_actor_methods(self, cid, version),
                BuiltinActor::Reward => reward::register_actor_methods(self, cid, version),
                _ => {}
            }
        }
    }
}

pub fn deserialize_params(
    code_cid: &Cid,
    method_num: MethodNum,
    params_bytes: &[u8],
) -> Result<Option<Value>> {
    METHOD_REGISTRY.deserialize_params(code_cid, method_num, params_bytes)
}

macro_rules! register_actor_methods {
    // Handle an empty params case
    ($registry:expr, $code_cid:expr, [
        $( ($method:expr, empty) ),* $(,)?
    ]) => {
        $(
            $registry.register_method(
                $code_cid,
                $method as MethodNum,
                |bytes| -> anyhow::Result<serde_json::Value> {
                    if bytes.is_empty() {
                        Ok(serde_json::json!({}))
                    } else {
                        Ok(fvm_ipld_encoding::from_slice(bytes)?)
                    }
                },
            );
        )*
    };

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

#[cfg(test)]
mod test {
    use super::*;
    use crate::lotus_json::HasLotusJson;
    use crate::utils::multihash::MultihashCode;
    use fvm_ipld_encoding::{DAG_CBOR, to_vec};
    use multihash_derive::MultihashDigest;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    const V16: u64 = 16;
    // Test parameter type for testing
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestParams {
        pub value: u64,
        pub message: String,
    }

    impl HasLotusJson for TestParams {
        type LotusJson = Self;

        #[cfg(test)]
        fn snapshots() -> Vec<(Value, Self)> {
            todo!()
        }

        fn into_lotus_json(self) -> Self::LotusJson {
            self
        }

        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            lotus_json
        }
    }

    fn create_test_cid(data: &[u8]) -> Cid {
        Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(data))
    }

    fn get_real_actor_cid(target_actor: BuiltinActor, target_version: u64) -> Option<Cid> {
        ACTOR_REGISTRY
            .iter()
            .find_map(|(&cid, &(actor_type, version))| {
                (actor_type == target_actor && version == target_version).then_some(cid)
            })
    }
    #[test]
    fn test_method_registry_initialization() {
        let result = deserialize_params(&create_test_cid(b"unknown"), 1, &[]);

        // Should fail with a specific error message about an unknown actor
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unknown actor code CID"));
    }

    #[test]
    fn test_deserialize_params_register_method() {
        let mut registry = MethodRegistry::new();
        let test_cid = create_test_cid(b"test_actor");
        let method_num = 42;

        // Register a test method
        registry.register_method(test_cid, method_num, |bytes| -> Result<TestParams> {
            Ok(fvm_ipld_encoding::from_slice(bytes)?)
        });

        let test_params = TestParams {
            value: 123,
            message: "test message".to_string(),
        };
        let encoded = to_vec(&test_params).unwrap();

        let result = registry.deserialize_params(&test_cid, method_num, &encoded);
        assert!(result.is_ok());

        let json_value = result.unwrap().unwrap();
        let expected_json = json!({
            "value": 123,
            "message": "test message"
        });
        assert_eq!(json_value, expected_json);
    }

    #[test]
    fn test_deserialize_params_unregistered_method() {
        let registry = MethodRegistry::new();
        let unknown_cid = create_test_cid(b"unknown_actor");
        let method_num = 99;

        let result = registry.deserialize_params(&unknown_cid, method_num, &[]);
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unknown actor code CID"));
    }

    #[test]
    fn test_deserialize_params_registered_actor_unregistered_method() {
        if let Some(account_cid) = get_real_actor_cid(BuiltinActor::Account, V16) {
            let unregistered_method = 999;

            let result = deserialize_params(&account_cid, unregistered_method, &[]);
            assert!(result.is_err());

            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("No deserializer registered for actor type"));
            assert!(error_msg.contains("Account"));
            assert!(error_msg.contains("method 999"));
        }
    }

    #[test]
    fn test_supported_actor_methods_registered() {
        // List of actor types that should have methods registered in the method registry
        let supported_actors = vec![
            BuiltinActor::Account,
            BuiltinActor::Miner,
            BuiltinActor::EVM,
        ];

        for actor_type in supported_actors {
            let actor_cid = get_real_actor_cid(actor_type, V16).unwrap();
            // Test that the Constructor method (typically method 1) is registered
            let constructor_method = 1;

            // Even with empty parameters, it should attempt deserialization rather than
            // returning the "no deserializer registered" error
            let result = deserialize_params(&actor_cid, constructor_method, &[]);

            if let Err(e) = result {
                let error_msg = e.to_string();
                assert!(
                    !error_msg.contains("No deserializer registered"),
                    "Actor type {actor_type:?} should have methods registered but got error: {error_msg}"
                );
            }
        }
    }

    #[test]
    fn test_register_actor_methods_macro() {
        let mut registry = MethodRegistry::new();
        let test_cid = create_test_cid(b"macro_test");

        // Test the register_actor_methods! macro
        register_actor_methods!(registry, test_cid, [(1, TestParams), (2, TestParams),]);

        let test_params = TestParams {
            value: 789,
            message: "macro test".to_string(),
        };
        let encoded = to_vec(&test_params).unwrap();

        // Verify methods were registered
        // Test method 1
        let result1 = registry.deserialize_params(&test_cid, 1, &encoded);
        assert!(result1.is_ok());

        // Test method 2
        let result2 = registry.deserialize_params(&test_cid, 2, &encoded);
        assert!(result2.is_ok());

        // Test unregistered method 3
        let result3 = registry.deserialize_params(&test_cid, 3, &encoded);
        assert!(result3.is_err());
    }

    #[test]
    fn test_system_actor_deserialize_params_cbor_null() {
        let system_cid = get_real_actor_cid(BuiltinActor::System, V16)
            .expect("Should have System actor CID in registry");

        // Test with null data
        let result = deserialize_params(&system_cid, 1, &[]);

        assert!(result.is_ok(), "Should handle CBOR null: {result:?}");
    }
}

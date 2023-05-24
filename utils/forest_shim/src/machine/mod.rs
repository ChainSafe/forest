// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub use fvm::machine::{
    DefaultMachine, Engine as Engine_v2, Machine, Manifest as ManifestV2,
    MultiEngine as MultiEngine_v2, NetworkConfig as NetworkConfig_v2,
};
pub use fvm3::{
    engine::{Engine as Engine_v3, EnginePool, MultiEngine as MultiEngine_v3},
    machine::{
        DefaultMachine as DefaultMachine_v3, Machine as Machine_v3,
        NetworkConfig as NetworkConfig_v3,
    },
};
mod manifest_v3;
pub use manifest_v3::ManifestV3;

pub enum MultiEngineVersion {
    V2,
    V3,
}

pub enum MultiEngine {
    V2(MultiEngine_v2),
    V3(MultiEngine_v3),
}

pub enum Engine {
    V2(Engine_v2),
    V3(Engine_v3),
}

#[derive(Clone)]
pub enum NetworkConfig {
    V2(NetworkConfig_v2),
    V3(NetworkConfig_v3),
}

pub enum EngineReturn {
    V2(Engine),
    V3(EnginePool),
}

impl From<MultiEngine_v2> for MultiEngine {
    fn from(other: MultiEngine_v2) -> Self {
        MultiEngine::V2(other)
    }
}

impl From<MultiEngine> for MultiEngine_v2 {
    fn from(other: MultiEngine) -> Self {
        match other {
            MultiEngine::V2(multi_engine) => multi_engine,
            MultiEngine::V3(multi_engine) => MultiEngine::V3(multi_engine).into(),
        }
    }
}

impl From<MultiEngine> for MultiEngine_v3 {
    fn from(other: MultiEngine) -> Self {
        match other {
            MultiEngine::V2(multi_engine) => MultiEngine::V2(multi_engine).into(),
            MultiEngine::V3(multi_engine) => multi_engine,
        }
    }
}

impl From<MultiEngine_v3> for MultiEngine {
    fn from(other: MultiEngine_v3) -> Self {
        MultiEngine::V3(other)
    }
}

impl From<Engine_v2> for Engine {
    fn from(other: Engine_v2) -> Self {
        Engine::V2(other)
    }
}

impl From<Engine_v3> for Engine {
    fn from(other: Engine_v3) -> Self {
        Engine::V3(other)
    }
}

impl From<&NetworkConfig> for NetworkConfig_v2 {
    fn from(other: &NetworkConfig) -> Self {
        match other.clone() {
            NetworkConfig::V2(network_config) => network_config,
            NetworkConfig::V3(network_config) => NetworkConfig::V3(network_config).into(),
        }
    }
}

impl From<&NetworkConfig> for NetworkConfig_v3 {
    fn from(other: &NetworkConfig) -> Self {
        match other.clone() {
            NetworkConfig::V2(network_config) => NetworkConfig::V2(network_config).into(),
            NetworkConfig::V3(network_config) => network_config,
        }
    }
}

impl From<NetworkConfig> for NetworkConfig_v2 {
    fn from(other: NetworkConfig) -> Self {
        NetworkConfig::V2(other.into()).into()
    }
}

impl From<NetworkConfig> for NetworkConfig_v3 {
    fn from(other: NetworkConfig) -> Self {
        NetworkConfig::V3(other.into()).into()
    }
}

impl From<Engine_v2> for EngineReturn {
    fn from(other: Engine_v2) -> Self {
        EngineReturn::V2(other.into())
    }
}

impl From<EnginePool> for EngineReturn {
    fn from(other: EnginePool) -> Self {
        EngineReturn::V3(other.into())
    }
}

impl MultiEngine {
    pub fn new(version: MultiEngineVersion, concurrency: Option<u32>) -> MultiEngine {
        match version {
            MultiEngineVersion::V2 => MultiEngine_v2::new().into(),
            MultiEngineVersion::V3 => MultiEngine_v3::new(concurrency.unwrap_or(1)).into(),
        }
    }

    pub fn get(&self, nc: &NetworkConfig) -> anyhow::Result<EngineReturn> {
        match self {
            MultiEngine::V2(v2) => Ok(MultiEngine_v2::get(v2, &nc.into())?.into()),
            MultiEngine::V3(v3) => Ok(MultiEngine_v3::get(v3, &nc.into())?.into()),
        }
    }
}

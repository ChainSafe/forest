// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub use fvm::machine::{
    DefaultMachine, Engine as Engine_v2, Machine, Manifest as ManifestV2, MultiEngine as MultiEngine_v2, NetworkConfig as NetworkConfig_v2,
};
pub use fvm3::{
    engine::{EnginePool, Engine as Engine_v3, MultiEngine as MultiEngine_v3},
    machine::{
        DefaultMachine as DefaultMachine_v3, Machine as Machine_v3,
        NetworkConfig as NetworkConfig_v3,
    },
};
mod manifest_v3;
pub use manifest_v3::ManifestV3;

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
    V3(EnginePool)
}

impl From<MultiEngine_v2> for MultiEngine {
    fn from(other: MultiEngine_v2) -> Self {
        MultiEngine::V2(other)
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
            NetworkConfig::V3(network_config) => NetworkConfig::V3(network_config).into()
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
    // TODO: may be able to use the strategy used for StateTree to fix this
    pub fn new(&self, concurrency: Option<u32>) -> MultiEngine {
        match self {
            MultiEngine::V2(_) => MultiEngine_v2::new().into(),
            MultiEngine::V3(_) => MultiEngine_v3::new(concurrency.unwrap_or(1)).into(),            
        }
    }

    pub fn get(&self, nc: &NetworkConfig) -> anyhow::Result<EngineReturn>  {
        match self {
            MultiEngine::V2(v2) => Ok(MultiEngine_v2::get(v2, &nc.into())?.into()),
            MultiEngine::V3(v3) => Ok(MultiEngine_v3::get(v3, &nc.into())?.into()), 
        }
    }
}

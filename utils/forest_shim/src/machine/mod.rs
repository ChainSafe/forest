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

pub struct MultiEngine {
    pub v2: MultiEngine_v2,
    pub v3: MultiEngine_v3,
}

#[derive(Clone)]
pub enum NetworkConfig {
    V2(NetworkConfig_v2),
    V3(NetworkConfig_v3),
}

pub struct EngineReturn {
    _v2: Result<Engine_v2, anyhow::Error>,
    _v3: Result<EnginePool, anyhow::Error>,
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

impl MultiEngine {
    pub fn new(concurrency: Option<u32>) -> MultiEngine {
        MultiEngine {
            v2: MultiEngine_v2::new(),
            v3: MultiEngine_v3::new(concurrency.unwrap_or(1)), // `1` is default concurrency value in `fvm3`
        }
    }

    pub fn get(&self, nc: &NetworkConfig) -> EngineReturn {
        EngineReturn {
            _v2: MultiEngine_v2::get(&self.v2, &nc.into()),
            _v3: MultiEngine_v3::get(&self.v3, &nc.into()),
        }
    }
}

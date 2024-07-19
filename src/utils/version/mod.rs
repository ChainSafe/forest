// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use git_version::git_version;
use once_cell::sync::Lazy;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::info::Info,
};

/// Current git commit hash of the Forest repository.
pub const GIT_HASH: &str =
    git_version!(args = ["--always", "--exclude", "*"], fallback = "unknown");

/// Current version of the Forest repository with git hash embedded
/// E.g., `0.8.0+git.e69baf3e4`
pub static FOREST_VERSION_STRING: Lazy<String> =
    Lazy::new(|| format!("{}+git.{}", env!("CARGO_PKG_VERSION"), GIT_HASH));

pub static FOREST_VERSION: Lazy<semver::Version> =
    Lazy::new(|| semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("Invalid version"));

#[derive(Debug)]
pub struct ForestVersionCollector {
    version: Info<Vec<(&'static str, &'static str)>>,
}

impl ForestVersionCollector {
    pub fn new() -> Self {
        let version = Info::new(vec![("version", FOREST_VERSION_STRING.as_str())]);
        Self { version }
    }
}

impl Collector for ForestVersionCollector {
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        let metric_encoder = encoder.encode_descriptor(
            "forest_version",
            "semantic version of the forest binary",
            None,
            self.version.metric_type(),
        )?;
        self.version.encode(metric_encoder)?;
        Ok(())
    }
}

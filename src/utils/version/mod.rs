// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use git_version::git_version;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeLabelSet, EncodeMetric},
    metrics::{family::Family, gauge::Gauge},
};
use std::sync::LazyLock;

/// Current git commit hash of the Forest repository.
pub const GIT_HASH: &str =
    git_version!(args = ["--always", "--exclude", "*"], fallback = "unknown");

/// Current version of the Forest repository with git hash embedded
/// E.g., `0.8.0+git.e69baf3e4`
pub static FOREST_VERSION_STRING: LazyLock<String> =
    LazyLock::new(|| format!("{}+git.{}", env!("CARGO_PKG_VERSION"), GIT_HASH));

/// `User-Agent` advertised by Forest's HTTP clients, e.g. `forest/0.33.7+git.e69baf3e4`.
///
/// Some public RPC providers (e.g. Filfox, fronted by Cloudflare) reject requests
/// without a `User-Agent` with a `403`.
pub const FOREST_USER_AGENT: &str = git_version!(
    args = ["--always", "--exclude", "*"],
    prefix = concat!("forest/", env!("CARGO_PKG_VERSION"), "+git."),
    fallback = concat!("forest/", env!("CARGO_PKG_VERSION"), "+git.unknown")
);

pub static FOREST_VERSION: LazyLock<semver::Version> =
    LazyLock::new(|| semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("Invalid version"));

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet, derive_more::Constructor)]
pub struct VersionLabel {
    version: &'static str,
}

#[derive(Debug)]
pub struct ForestVersionCollector {
    version: Family<VersionLabel, Gauge>,
}

impl ForestVersionCollector {
    pub fn new() -> Self {
        Self {
            version: Family::default(),
        }
    }
}

impl Collector for ForestVersionCollector {
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        let metric_encoder = encoder.encode_descriptor(
            "build_info",
            "semantic version of the forest binary",
            None,
            self.version.metric_type(),
        )?;
        self.version
            .get_or_create(&VersionLabel::new(FOREST_VERSION_STRING.as_str()))
            .set(1);
        self.version.encode(metric_encoder)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_matches_version_string() {
        assert_eq!(
            FOREST_USER_AGENT,
            format!("forest/{}", *FOREST_VERSION_STRING)
        );
        http::HeaderValue::from_str(FOREST_USER_AGENT).unwrap();
    }
}

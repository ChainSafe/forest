// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use lazy_static::lazy_static;
use num_derive::FromPrimitive;
use serde::Serialize;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::process::Command;

#[cfg(not(feature = "release"))]
const RELEASE_TRACK: &str = "unstable";

#[cfg(feature = "release")]
const RELEASE_TRACK: &str = "alpha";

// masks
const MINOR_MASK: u32 = 0xffff00;
const MAJOR_ONLY_MASK: u32 = 0xff0000;
const MINOR_ONLY_MASK: u32 = 0x00ff00;
const PATCH_ONLY_MASK: u32 = 0x0000ff;

// api versions
const FULL_API_VERSION: Version = new_version(1, 1, 0);
const MINER_API_VERSION: Version = new_version(0, 15, 0);
const WORKER_API_VERSION: Version = new_version(0, 15, 0);

lazy_static! {
    pub static ref RUNNING_NODE_TYPE: RwLock<NodeType> = RwLock::new(NodeType::Full);
}

/// Represents the current version of the API.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct APIVersion {
    pub version: String,
    pub api_version: Version,
    pub block_delay: u64,
}

/// Integer based value on version information. Highest order bits for Major, Mid order for Minor
/// and lowest for Patch.
#[derive(Serialize)]
pub struct Version(u32);

/// The type of node that is running.
#[derive(FromPrimitive, Debug)]
#[repr(u64)]
pub enum NodeType {
    Unknown = 0,
    Full = 1,
    Miner = 2,
    Worker = 3,
}

impl Display for NodeType {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:?}", self)
    }
}

const fn new_version(major: u32, minor: u32, patch: u32) -> Version {
    Version(major << 16 | minor << 8 | patch)
}

/// Gets the formatted current user version.
pub async fn user_version() -> String {
    option_env!("FOREST_VERSION")
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string()
}

impl Version {
    fn ints(&self) -> (u32, u32, u32) {
        let v = self.0;
        (
            (v & MAJOR_ONLY_MASK) >> 16,
            (v & MINOR_ONLY_MASK) >> 8,
            v & PATCH_ONLY_MASK,
        )
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.0 & MINOR_MASK == other.0 & MINOR_MASK
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let (major, minor, patch) = self.ints();
        write!(f, "{}.{}.{}", major, minor, patch)
    }
}

impl std::convert::TryFrom<&NodeType> for Version {
    type Error = String;
    fn try_from(node_type: &NodeType) -> Result<Self, Self::Error> {
        match node_type {
            NodeType::Full => Ok(FULL_API_VERSION),
            NodeType::Miner => Ok(MINER_API_VERSION),
            NodeType::Worker => Ok(WORKER_API_VERSION),
            _ => Err(format!("unknown node type {}", node_type)),
        }
    }
}

/// Returns version string at build time, e.g., `0.2.2-unstable+git.21146f40`
pub fn version() -> String {
    // FIXME: this is no good because crate version (so fil_types) will be
    // taken and not forest version one.
    let git_hash = match Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
    {
        Ok(output) => String::from_utf8(output.stdout).unwrap_or_default(),
        _ => "unknown".to_owned(),
    };
    // TODO: add network name when possible, ie +mainnet, +calibnet, etc
    format!(
        "{}-{}+git.{}",
        env!("CARGO_PKG_VERSION"),
        RELEASE_TRACK,
        git_hash,
    )
}

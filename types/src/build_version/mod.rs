// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use git_version::git_version;
use lazy_static::lazy_static;
use num_derive::FromPrimitive;
use serde::Serialize;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[cfg(debug_assertions)]
const DEBUG_BUILD: &str = "+debug";

#[cfg(not(debug_assertions))]
const DEBUG_BUILD: &str = "";

// masks
const MINOR_MASK: u32 = 0xffff00;
const MAJOR_ONLY_MASK: u32 = 0xff0000;
const MINOR_ONLY_MASK: u32 = 0x00ff00;
const PATCH_ONLY_MASK: u32 = 0x0000ff;

// api versions
const FULL_API_VERSION: Version = Version::new(1, 1, 0);
const MINER_API_VERSION: Version = Version::new(0, 15, 0);
const WORKER_API_VERSION: Version = Version::new(0, 15, 0);

const GIT_HASH: &str = git_version!(args = ["--always", "--exclude", "*"], fallback = "unknown");

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

/// Gets the formatted current user version.
pub fn user_version() -> String {
    option_env!("FOREST_VERSION")
        .unwrap_or("unknown")
        .to_string()
}

impl Version {
    const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self(major << 16 | minor << 8 | patch)
    }

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

/// Returns the version string, e.g., `0.2.2+debug+git.e2e5b9d1`
pub fn version(pkg_version: &str) -> String {
    format!("{}{}+git.{}", pkg_version, DEBUG_BUILD, GIT_HASH)
}

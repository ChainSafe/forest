// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use git_version::git_version;
use num_derive::FromPrimitive;
use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::Serialize;
const BUILD_VERSION: &str = "0.10.2";

// masks
const MINOR_MASK: u32 = 0xffff00;
const MAJOR_ONLY_MASK: u32 = 0xff0000;
const MINOR_ONLY_MASK: u32 = 0x00ff00;
const PATCH_ONLY_MASK: u32 = 0x0000ff;

// api versions
const FULL_API_VERSION: Version = new_version(1, 0, 0);
const MINER_API_VERSION: Version = new_version(0, 15, 0);
const WORKER_API_VERSION: Version = new_version(0, 15, 0);

lazy_static! {
    pub static ref CURRENT_COMMIT: String = git_version!(fallback = "unknown").to_string();
    pub static ref BUILD_TYPE: RwLock<BuildType> = RwLock::new(BuildType::BuildDefault);
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

/// Build type for the node. This shares which build type the node is from the RPC API.
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum BuildType {
    BuildDefault = 0x0,
    Build2k = 0x1,
    BuildDebug = 0x2,
}

/// The type of node that is running.
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum NodeType {
    Unknown = 0,
    Full = 1,
    Miner = 2,
    Worker = 3,
}

impl Display for NodeType {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self)
    }
}

impl BuildType {
    fn to_str(&self) -> &str {
        match self {
            BuildType::BuildDefault => "",
            BuildType::Build2k => "+debug",
            BuildType::BuildDebug => "+2k",
        }
    }
}

const fn new_version(major: u32, minor: u32, patch: u32) -> Version {
    Version(major << 16 | minor << 8 | patch)
}

/// Gets the formatted current user version.
pub async fn user_version() -> String {
    BUILD_VERSION.to_owned() + &*BUILD_TYPE.read().await.to_str() + &CURRENT_COMMIT
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

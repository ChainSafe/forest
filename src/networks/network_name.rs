// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! There are two concepts of "network name" in Forest:
//! 1. [`GenesisNetworkName`]: The network name as defined in the genesis block.
//!    This is not necessarily the same as the network name that the node is
//!    currently on.
//! 2. [`StateNetworkName`]: The network name as defined by the state of the node.
//!    This is the network name that the node is currently on.

/// The network name as defined in the genesis block.
/// This is not necessarily the same as the network name that the node is
/// currently on. This is used by `libp2p` layer and the message pool.
pub struct GenesisNetworkName(String);

impl AsRef<str> for GenesisNetworkName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GenesisNetworkName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for GenesisNetworkName {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

impl From<String> for GenesisNetworkName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl From<GenesisNetworkName> for String {
    fn from(name: GenesisNetworkName) -> Self {
        name.0
    }
}

/// The network name as defined by the state of the node.
/// This is the network name that the node is currently on.
pub struct StateNetworkName(String);

impl AsRef<str> for StateNetworkName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for StateNetworkName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for StateNetworkName {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

impl From<String> for StateNetworkName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl From<StateNetworkName> for String {
    fn from(name: StateNetworkName) -> Self {
        name.0
    }
}

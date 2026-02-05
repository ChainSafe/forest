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
#[derive(derive_more::Display, derive_more::From, derive_more::Into, derive_more::AsRef)]
#[from(String, &str)]
pub struct GenesisNetworkName(String);

/// The network name as defined by the state of the node.
/// This is the network name that the node is currently on.
#[derive(derive_more::Display, derive_more::From, derive_more::Into, derive_more::AsRef)]
#[from(String, &str)]
pub struct StateNetworkName(String);

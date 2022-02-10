// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{MAINNET_PREFIX, TESTNET_PREFIX};

/// Network defines the preconfigured networks to use with address encoding
#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub enum Network {
    Mainnet,
    Testnet,
}

impl From<Network> for fvm_shared::address::Network {
    fn from(network: Network) -> Self {
        match network {
            Network::Mainnet => fvm_shared::address::Network::Mainnet,
            Network::Testnet => fvm_shared::address::Network::Testnet,
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Network::Mainnet
    }
}

impl Network {
    /// to_prefix is used to convert the network into a string
    /// used when converting address to string
    pub(super) fn to_prefix(self) -> &'static str {
        match self {
            Network::Mainnet => MAINNET_PREFIX,
            Network::Testnet => TESTNET_PREFIX,
        }
    }
}

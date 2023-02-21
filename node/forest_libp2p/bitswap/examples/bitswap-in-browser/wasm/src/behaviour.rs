// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_libp2p_bitswap::BitswapBehaviour;
use libp2p::swarm::{keep_alive, NetworkBehaviour};

#[derive(NetworkBehaviour, Default)]
pub struct DemoBehaviour {
    pub bitswap: BitswapBehaviour,
    pub keep_alive: keep_alive::Behaviour,
}

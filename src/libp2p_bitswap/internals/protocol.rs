// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::request_response::ProtocolName;

#[derive(Debug, Clone)]
pub struct BitswapProtocol(pub &'static [u8]);

impl ProtocolName for BitswapProtocol {
    fn protocol_name(&self) -> &[u8] {
        self.0
    }
}

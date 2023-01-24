// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{codec::*, protocol::*, *};
use libp2p::{
    request_response::{ProtocolSupport, RequestId, RequestResponse, RequestResponseConfig},
    swarm::NetworkBehaviour,
    PeerId,
};

#[derive(NetworkBehaviour)]
pub struct BitswapBehaviour {
    pub(super) inner: RequestResponse<BitswapRequestResponseCodec>,
}

impl BitswapBehaviour {
    pub fn new(protocols: &[&'static [u8]], cfg: RequestResponseConfig) -> Self {
        assert!(!protocols.is_empty(), "protocols cannot be empty");

        let protocols: Vec<_> = protocols
            .iter()
            .map(|&n| (BitswapProtocol(n), ProtocolSupport::Full))
            .collect();
        BitswapBehaviour {
            inner: RequestResponse::new(BitswapRequestResponseCodec, protocols, cfg),
        }
    }

    pub fn send_request(&mut self, peer: &PeerId, request: BitswapRequest) -> RequestId {
        match request.ty {
            RequestType::Have => metrics::message_counter_outbound_request_have().inc(),
            RequestType::Block => metrics::message_counter_outbound_request_block().inc(),
        }
        self.inner
            .send_request(peer, vec![BitswapMessage::Request(request)])
    }

    pub fn send_response(&mut self, peer: &PeerId, response: (Cid, BitswapResponse)) -> RequestId {
        match response.1 {
            BitswapResponse::Have(..) => metrics::message_counter_outbound_response_have().inc(),
            BitswapResponse::Block(..) => metrics::message_counter_outbound_response_block().inc(),
        }
        self.inner
            .send_request(peer, vec![BitswapMessage::Response(response.0, response.1)])
    }
}

impl Default for BitswapBehaviour {
    fn default() -> Self {
        // This matches default values in `go-bitswap`
        BitswapBehaviour::new(
            &[
                b"/ipfs/bitswap/1.2.0",
                b"/ipfs/bitswap/1.1.0",
                b"/ipfs/bitswap/1.0.0",
                b"/ipfs/bitswap",
            ],
            Default::default(),
        )
    }
}

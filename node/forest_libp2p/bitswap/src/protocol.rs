use std::fmt::Display;

use libp2p::request_response::ProtocolName;

#[derive(Debug, Clone)]
pub struct BitswapProtocol(pub &'static [u8]);

impl ProtocolName for BitswapProtocol {
    fn protocol_name(&self) -> &[u8] {
        self.0
    }
}

impl Display for BitswapProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(self.0))
    }
}

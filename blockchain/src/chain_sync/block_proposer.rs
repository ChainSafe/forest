use super::BlockMsg;
use std::io;
/// BlockProposer allows callers to propose new blocks for inclusion in the chain
pub trait BlockProposer {
    fn send_hello(&self, bm: BlockMsg) -> Result<(), io::Error>;
    fn send_own_block(&self, bm: BlockMsg) -> Result<(), io::Error>;
    fn send_gossip_block(&self, bm: BlockMsg) -> Result<(), io::Error>;
}

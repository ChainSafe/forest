use crate::blocks::chain_info::BlockMsg;
use std::io;
/// BlockProposer allows callers to propose new blocks for inclusion in the chain
trait BlockProposer {
    fn send_hello(bm: BlockMsg) -> Result<(), io::Error>;
    fn send_own_block(bm: BlockMsg) -> Result<(), io::Error>;
    fn send_gossip_block(bm: BlockMsg) -> Result<(), io::Error>;
}

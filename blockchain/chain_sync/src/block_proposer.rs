use std::io;

trait BlockProposer {
  fn send_hello(&self) -> Result<(), io::Error>;
  // fn send_own_block(&self) -> Result<(), io:Error>;
  // fn send_gossip_block(&self) -> Result<(), io:Error>;
}

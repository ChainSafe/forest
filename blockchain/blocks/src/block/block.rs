use crate::block::block_header::{BlockHeader};
use message::{SignedMessage};

struct Block {
    header: BlockHeader,
    messages: Vec<SignedMessage>,
    //receipts: Vec<MessageReceipt>
}
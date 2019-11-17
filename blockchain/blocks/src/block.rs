use crate::block_header::BlockHeader;
use message::SignedMessage;

#[allow(dead_code)]
struct Block {
    header: BlockHeader,
    messages: Vec<SignedMessage>,
    //receipts: Vec<MessageReceipt>
}

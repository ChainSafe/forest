extern crate cid;
use cid::Cid;

pub type UVarint = u64;

pub type TokenAmount = UVarint;
pub type CallSeqNum = UVarint;

pub type CodeCID = Cid;
pub type ActorSubstateCID = Cid;

pub struct ActorState {
    pub code_cid: CodeCID,
    pub state: ActorSubstateCID,
    pub balance: TokenAmount,
    pub call_seq_num: CallSeqNum,
}

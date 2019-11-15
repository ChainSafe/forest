extern crate cid;
use cid::Cid;

//Might have to be some bignum
pub type UVarint = u64;

pub type TokenAmount = UVarint;
pub type CallSeqNum = UVarint;

pub type CodeCID = Cid;
pub type ActorSubstateCID = Cid;

pub struct ActorState {
    code_cid: CodeCID,
    state: ActorSubstateCID,
    balance: TokenAmount,
    call_seq_num: CallSeqNum,
}

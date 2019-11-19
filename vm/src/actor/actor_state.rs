extern crate cid;
use cid::Cid;

#[allow(dead_code)]
pub type UVarint = u64;

#[allow(dead_code)]
pub type TokenAmount = UVarint;
#[allow(dead_code)]
pub type CallSeqNum = UVarint;

#[allow(dead_code)]
pub type CodeCID = Cid;
#[allow(dead_code)]
pub type ActorSubstateCID = Cid;

#[allow(dead_code)]
pub struct ActorState {
    code_cid: CodeCID,
    state: ActorSubstateCID,
    balance: TokenAmount,
    call_seq_num: CallSeqNum,
}

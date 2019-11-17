extern crate cid;
use cid::Cid;

#[allow(dead_code)]
type UVarint = u64;

#[allow(dead_code)]
type TokenAmount = UVarint;
#[allow(dead_code)]
type CallSeqNum = UVarint;

#[allow(dead_code)]
type CodeCID = Cid;
#[allow(dead_code)]
type ActorSubstateCID = Cid;

#[allow(dead_code)]
struct ActorState {
    code_cid: CodeCID,
    state: ActorSubstateCID,
    balance: TokenAmount,
    call_seq_num: CallSeqNum,
}

extern crate cid;
use cid::{Cid};

//Might have to be some bignum
pub type UVarint = u64;

pub type TokenAmount = UVarint;
pub type CallSeqNum = UVarint;


pub type CodeCID = Cid;
pub type ActorSubstateCID = Cid;

pub struct ActorState {
    codeCID: CodeCID,
    state: ActorSubstateCID,
    balance: TokenAmount,
    callSeqNum: CallSeqNum,
}


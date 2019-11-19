extern crate cid;
use cid::{Cid};
use bytes::{Bytes};

//Might have to be some bignum
pub type UVarint = u64;

pub type MethodNum = UVarint;
pub type MethodParam = Bytes;
pub type MethodParams = Vec<MethodParam>; 
pub type Code = Bytes;

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

pub struct Actor {
    pub state: ActorState,
}
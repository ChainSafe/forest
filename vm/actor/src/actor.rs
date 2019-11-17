use crate::actor_state::{ActorState, UVarint};

use bytes::Bytes;

pub type MethodNum = UVarint;
pub type MethodParam = Bytes;
pub type MethodParams = Vec<MethodParam>;
pub type Code = Bytes;

#[allow(dead_code)]
pub struct Actor {
    state: ActorState,
}

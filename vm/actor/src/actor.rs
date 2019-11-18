use crate::actor_state::{ActorState, UVarint};

pub type MethodNum = UVarint;
pub type MethodParam = Vec<u8>;
pub type MethodParams = Vec<MethodParam>;
pub type Code = Vec<u8>;

#[allow(dead_code)]
pub struct Actor {
    state: ActorState,
}

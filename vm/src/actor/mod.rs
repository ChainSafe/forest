mod actor_state;

pub use actor_state::*;

pub type MethodNum = UVarint;
pub type MethodParam = Vec<u8>;
pub type MethodParams = Vec<MethodParam>;
pub type Code = Vec<u8>;

#[allow(dead_code)]
pub struct Actor {
    state: ActorState,
}

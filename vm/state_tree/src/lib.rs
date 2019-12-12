use actor::{ActorState, CodeID};
use address::Address;
use vm::TokenAmount;

pub trait StateTree {
    fn get_actor_address(&self, n: CodeID) -> Address;
    fn get_actor_state(&self, n: CodeID) -> ActorState;
    fn balance(&self, a: Address) -> TokenAmount;
}

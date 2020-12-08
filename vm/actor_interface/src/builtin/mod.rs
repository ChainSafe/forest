pub mod init;
pub mod system;
pub mod account;
pub mod miner;

use cid::Cid;

/// Returns true if the code belongs to a builtin actor.
pub fn is_builtin_actor(code: &Cid) -> bool {
    actorv0::is_builtin_actor(code) || actorv2::is_builtin_actor(code)
}

/// Returns true if the code belongs to an account actor.
pub fn is_account_actor(code: &Cid) -> bool {
    actorv0::is_account_actor(code) || actorv2::is_account_actor(code)
}

/// Returns true if the code belongs to a singleton actor.
pub fn is_singleton_actor(code: &Cid) -> bool {
    actorv0::is_singleton_actor(code) || actorv2::is_singleton_actor(code)
}

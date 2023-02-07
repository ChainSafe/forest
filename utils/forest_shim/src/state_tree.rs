// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::ops::{Deref, DerefMut};

use fvm::state_tree::ActorState as ActorStateV2;
/// Re-use `StateTree` from FVM2 directly, without wrapping. Moving forward, we
/// should use `StateTree` from FVM3. Unfortunately, for the time being, we are
/// blocked by lack of migrations.
pub use fvm::state_tree::StateTree;
use fvm3::state_tree::ActorState as ActorStateV3;
use serde::{Deserialize, Serialize};

use crate::{econ::TokenAmount, Inner};

/// Newtype to wrap different versions of `fvm::state_tree::ActorState`
///
/// # Examples
/// ```
/// use cid::Cid;
///
/// // Create FVM2 ActorState normally
/// let fvm2_actor_state = fvm::state_tree::ActorState::new(Cid::default(), Cid::default(),
/// fvm_shared::econ::TokenAmount::from_atto(42), 0);
///
/// // Create a correspndoning FVM3 ActorState
/// let fvm3_actor_state = fvm3::state_tree::ActorState::new(Cid::default(), Cid::default(),
/// fvm_shared3::econ::TokenAmount::from_atto(42), 0, None);
///
/// // Create a shim out of fvm2 state, ensure conversions are correct
/// let state_shim = forest_shim::state_tree::ActorState::from(fvm2_actor_state.clone());
/// assert_eq!(fvm3_actor_state, *state_shim);
/// assert_eq!(fvm2_actor_state, state_shim.into());
/// ```
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActorState(ActorStateV3);

impl Inner for ActorState {
    type FVM = ActorStateV3;
}

impl Deref for ActorState {
    type Target = ActorStateV3;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ActorState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<ActorStateV3> for ActorState {
    fn from(value: ActorStateV3) -> Self {
        ActorState(value)
    }
}

impl From<ActorStateV2> for ActorState {
    fn from(value: ActorStateV2) -> Self {
        ActorState(ActorStateV3 {
            code: value.code,
            state: value.state,
            sequence: value.sequence,
            balance: TokenAmount::from(value.balance).into(),
            delegated_address: None,
        })
    }
}

impl From<ActorState> for ActorStateV3 {
    fn from(other: ActorState) -> Self {
        other.0
    }
}

impl From<ActorState> for ActorStateV2 {
    fn from(other: ActorState) -> ActorStateV2 {
        ActorStateV2 {
            code: other.code,
            state: other.state,
            sequence: other.sequence,
            balance: TokenAmount::from(&other.balance).into(),
        }
    }
}

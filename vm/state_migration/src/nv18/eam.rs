// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_shim::state_tree::ActorState;

use super::calibnet;

pub fn create_eam_actor() -> ActorState {
    ActorState::new_empty(*calibnet::v10::EAM, None)
}

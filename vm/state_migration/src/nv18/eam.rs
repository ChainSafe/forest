// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_shim::state_tree::ActorState;

use super::calibnet;

pub fn create_eam_actor(head: Cid) -> ActorState {
    ActorState::new(*calibnet::v10::EAM, head, Default::default(), 0, None)
}

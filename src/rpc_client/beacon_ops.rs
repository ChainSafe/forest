// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest};
use crate::beacon::beacon_entries::BeaconEntry;
use crate::rpc_api::beacon_api::*;

impl ApiInfo {
    pub fn beacon_get_entry_req(first: i64) -> RpcRequest<BeaconEntry> {
        RpcRequest::new(BEACON_GET_ENTRY, (first,))
    }
}

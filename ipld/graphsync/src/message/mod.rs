// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod proto;

use cid::Cid;
use forest_ipld::{selector::Selector, Ipld};
use std::collections::HashMap;

type Priority = i32;
type RequestID = i32;
type ResponseStatusCode = i32;
type ExtensionName = String;

/// Struct which contains all request data from a GraphSyncMessage.
pub struct GraphSyncRequest {
    id: RequestID,
    root: Cid,
    selector: Selector,
    priority: Priority,
    extensions: HashMap<ExtensionName, Vec<u8>>,
    is_cancel: bool,
    is_update: bool,
}

/// Struct which contains all response data from a GraphSyncMessage.
pub struct GraphSyncResponse {
    id: RequestID,
    status: ResponseStatusCode,
    extensions: Hashmap<ExtensionName, Vec<u8>>,
}

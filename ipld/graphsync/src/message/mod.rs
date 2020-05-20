// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod proto;

use cid::Cid;
use fnv::FnvHashMap;
use forest_encoding::{Cbor, Error as EncodingError};
use forest_ipld::selector::Selector;
use std::collections::HashMap;
use std::convert::TryFrom;

type Priority = i32;
type RequestID = i32;
type ResponseStatusCode = i32;
type ExtensionName = String;

/// Struct which contains all request data from a GraphSyncMessage.
pub struct GraphSyncRequest {
    id: RequestID,
    root: Cid,
    selector: Option<Selector>,
    priority: Priority,
    extensions: HashMap<ExtensionName, Vec<u8>>,
    is_cancel: bool,
    is_update: bool,
}

/// Struct which contains all response data from a GraphSyncMessage.
pub struct GraphSyncResponse {
    id: RequestID,
    status: ResponseStatusCode,
    extensions: HashMap<ExtensionName, Vec<u8>>,
}

/// Contains all requests and responses
pub struct GraphSyncMessage {
    // TODO revisit for if these needs to be ordered, or preserve the order from over the wire
    requests: FnvHashMap<RequestID, GraphSyncRequest>,
    responses: FnvHashMap<RequestID, GraphSyncResponse>,
    blocks: HashMap<Cid, Vec<u8>>,
}

impl GraphSyncMessage {
    /// Returns reference to requests hashmap.
    pub fn requests(&self) -> &FnvHashMap<RequestID, GraphSyncRequest> {
        &self.requests
    }
    /// Returns reference to responses hashmap.
    pub fn responses(&self) -> &FnvHashMap<RequestID, GraphSyncResponse> {
        &self.responses
    }
    /// Returns reference to blocks hashmap.
    pub fn blocks(&self) -> &HashMap<Cid, Vec<u8>> {
        &self.blocks
    }
    /// Adds a request to GraphSyncMessage requests.
    pub fn add_request(&mut self, request: GraphSyncRequest) {
        self.requests.insert(request.id, request);
    }
    /// Adds a response to GraphSyncMessage responses.
    pub fn add_response(&mut self, response: GraphSyncResponse) {
        self.responses.insert(response.id, response);
    }
    /// Add block to message.
    // TODO revisit block format, should be fine to be kept seperate, but may need to merge
    pub fn add_block(&mut self, cid: Cid, block: Vec<u8>) {
        self.blocks.insert(cid, block);
    }
    /// Returns true if empty GraphSyncMessage.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.requests.is_empty() && self.responses.is_empty()
    }
}

impl TryFrom<GraphSyncMessage> for proto::Message {
    type Error = EncodingError;
    fn try_from(msg: GraphSyncMessage) -> Result<Self, Self::Error> {
        let requests: protobuf::RepeatedField<_> = msg
            .requests
            .into_iter()
            .map(|(_, req)| {
                let selector = match &req.selector {
                    Some(s) => s.marshal_cbor()?,
                    None => Vec::new(),
                };
                Ok(proto::Message_Request {
                    id: req.id,
                    root: req.root.to_bytes(),
                    selector,
                    extensions: req.extensions,
                    priority: req.priority,
                    cancel: req.is_cancel,
                    update: req.is_update,
                    ..Default::default()
                })
            })
            .collect::<Result<_, Self::Error>>()?;

        let responses: protobuf::RepeatedField<_> = msg
            .responses
            .into_iter()
            .map(|(_, res)| proto::Message_Response {
                id: res.id,
                status: res.status,
                extensions: res.extensions,
                ..Default::default()
            })
            .collect();

        let data: protobuf::RepeatedField<_> = msg
            .blocks
            .into_iter()
            .map(|(cid, data)| proto::Message_Block {
                data,
                prefix: cid.prefix().to_bytes(),
                ..Default::default()
            })
            .collect();

        Ok(proto::Message {
            requests,
            responses,
            data,
            ..Default::default()
        })
    }
}

impl From<proto::Message> for GraphSyncMessage {
    fn from(_msg: proto::Message) -> Self {
        todo!()
    }
}

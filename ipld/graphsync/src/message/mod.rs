// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod proto;

use super::*;
use cid::{Cid, Prefix};
use fnv::FnvHashMap;
use forest_encoding::{Cbor, Error as EncodingError};
use forest_ipld::selector::Selector;
use std::collections::HashMap;
use std::convert::TryFrom;

/// The data associated with a new graphsync request.
#[derive(Debug, PartialEq, Clone)]
pub struct NewRequestPayload {
    pub root: Cid,
    pub selector: Selector,
    pub priority: Priority,
    pub extensions: Extensions,
}

/// Defines the data associated with each request type.
#[derive(Debug, PartialEq, Clone)]
pub enum Payload {
    New(NewRequestPayload),
    Update { extensions: Extensions },
    Cancel,
}

/// Struct which contains all request data from a GraphSyncMessage.
#[derive(Debug, PartialEq, Clone)]
pub struct GraphSyncRequest {
    pub id: RequestID,
    pub payload: Payload,
}

impl GraphSyncRequest {
    pub fn new(
        id: RequestID,
        root: Cid,
        selector: Selector,
        priority: Priority,
        extensions: Option<Extensions>,
    ) -> Self {
        Self {
            id,
            payload: Payload::New(NewRequestPayload {
                root,
                selector,
                priority,
                extensions: extensions.unwrap_or_default(),
            }),
        }
    }
    /// Generate a GraphSyncRequest to update an in progress request with extensions.
    // TODO revisit this interface later, a map as a parameter isn't very ergonomic
    pub fn update(id: RequestID, extensions: Extensions) -> Self {
        Self {
            id,
            payload: Payload::Update { extensions },
        }
    }
    /// Generate a GraphSyncRequest to cancel and in progress GraphSync request.
    pub fn cancel(id: RequestID) -> Self {
        Self {
            id,
            payload: Payload::Cancel,
        }
    }
}

/// Struct which contains all response data from a GraphSyncMessage.
#[derive(Debug, PartialEq, Clone)]
pub struct GraphSyncResponse {
    pub id: RequestID,
    pub status: ResponseStatusCode,
    pub extensions: Extensions,
}

impl GraphSyncResponse {
    pub fn new(id: RequestID, status: ResponseStatusCode, extensions: Option<Extensions>) -> Self {
        Self {
            id,
            status,
            extensions: extensions.unwrap_or_default(),
        }
    }
}

/// Contains all requests and responses
#[derive(Debug, Default, PartialEq, Clone)]
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
    pub fn insert_request(&mut self, request: GraphSyncRequest) {
        self.requests.insert(request.id, request);
    }
    /// Adds a response to GraphSyncMessage responses.
    pub fn insert_response(&mut self, response: GraphSyncResponse) {
        self.responses.insert(response.id, response);
    }
    /// Add block to message.
    // TODO revisit block format, should be fine to be kept separate, but may need to merge.
    pub fn insert_block(&mut self, cid: Cid, block: Vec<u8>) {
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
            .map(|(_, req)| match req.payload {
                Payload::New(NewRequestPayload {
                    root,
                    selector,
                    priority,
                    extensions,
                }) => Ok(proto::Message_Request {
                    id: req.id,
                    // Cid bytes format (not cbor encoded)
                    root: root.to_bytes(),
                    // Cbor encoded selector
                    selector: selector.marshal_cbor()?,
                    extensions,
                    priority,
                    ..Default::default()
                }),
                Payload::Update { extensions } => Ok(proto::Message_Request {
                    id: req.id,
                    update: true,
                    extensions,
                    ..Default::default()
                }),
                Payload::Cancel => Ok(proto::Message_Request {
                    id: req.id,
                    cancel: true,
                    ..Default::default()
                }),
            })
            .collect::<Result<_, Self::Error>>()?;

        let responses: protobuf::RepeatedField<_> = msg
            .responses
            .into_iter()
            .map(|(_, res)| proto::Message_Response {
                id: res.id,
                status: res.status.to_i32(),
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

impl TryFrom<proto::Message> for GraphSyncMessage {
    type Error = EncodingError;
    fn try_from(msg: proto::Message) -> Result<Self, Self::Error> {
        let requests: FnvHashMap<_, _> = msg
            .requests
            .into_iter()
            .map(|r| {
                if r.cancel {
                    Ok((r.id, GraphSyncRequest::cancel(r.id)))
                } else if r.update {
                    Ok((r.id, GraphSyncRequest::update(r.id, r.extensions)))
                } else {
                    Ok((
                        r.id,
                        GraphSyncRequest::new(
                            r.id,
                            Cid::try_from(r.root)?,
                            Selector::unmarshal_cbor(&r.selector)?,
                            r.priority,
                            Some(r.extensions),
                        ),
                    ))
                }
            })
            .collect::<Result<_, Self::Error>>()?;

        let responses: FnvHashMap<_, _> = msg
            .responses
            .into_iter()
            .map(|r| {
                (
                    r.id,
                    GraphSyncResponse {
                        id: r.id,
                        extensions: r.extensions,
                        status: ResponseStatusCode::from_i32(r.status),
                    },
                )
            })
            .collect();

        let blocks: HashMap<_, _> = msg
            .data
            .into_iter()
            .map(|block| {
                let prefix = Prefix::new_from_bytes(&block.prefix)?;
                let cid = Cid::new_from_prefix(&prefix, &block.data)?;
                Ok((cid, block.data))
            })
            .collect::<Result<_, Self::Error>>()?;

        Ok(GraphSyncMessage {
            requests,
            responses,
            blocks,
        })
    }
}

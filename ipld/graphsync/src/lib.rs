// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// TODO evaluate exporting from libp2p mod
pub mod libp2p;
mod message;
mod response_manager;

#[cfg(test)]
mod test_utils;

pub use self::message::*;

use cid::Cid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The maximum combined block size for a single message.
pub const MAX_BLOCK_SIZE: usize = 512 * 1024;

/// Provides response metadata for a GraphSync request,
/// see https://github.com/ipld/specs/blob/master/block-layer/graphsync/known_extensions.md
pub const EXTENSION_METADATA: &str = "graphsync/response-metadata";

/// Tells the responding peer not to send certain blocks if they are encountered in a traversal,
/// see https://github.com/ipld/specs/blob/master/block-layer/graphsync/known_extensions.md
pub const EXTENSION_DO_NOT_SEND_CIDS: &str = "graphsync/do-not-send-cids";

/// Priority for a GraphSync request.
pub type Priority = i32;
/// Unique identifier for a GraphSync request.
pub type RequestID = i32;
/// Name for a GraphSync extension.
pub type ExtensionName = String;
/// Represents the data attached as extensions to the requests.
pub type Extensions = HashMap<ExtensionName, Vec<u8>>;

/// A name/data pair for a GraphSync extension.
#[derive(Debug, Clone)]
pub struct ExtensionData {
    name: ExtensionName,
    data: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct MetadataItem {
    link: Cid,
    #[serde(rename = "blockPresent")]
    block_is_present: bool,
}

/// Status code returned for a GraphSync Request.
#[derive(PartialEq, Clone, Copy, Eq, Debug)]
pub enum ResponseStatusCode {
    // Informational Response Codes (partial)
    /// RequestAcknowledged means the request was received and is being worked on.
    RequestAcknowledged,
    /// AdditionalPeers means additional peers were found that may be able
    /// to satisfy the request and contained in the extra block of the response.
    AdditionalPeers,
    /// NotEnoughGas means fulfilling this request requires payment.
    NotEnoughGas,
    /// OtherProtocol means a different type of response than GraphSync is
    /// contained in extra.
    OtherProtocol,
    /// PartialResponse may include blocks and metadata about the in progress response
    /// in extra.
    PartialResponse,
    /// RequestPaused indicates a request is paused and will not send any more data
    /// until unpaused
    RequestPaused,

    // Success Response Codes (request terminated)
    /// RequestCompletedFull means the entire fulfillment of the GraphSync request
    /// was sent back.
    RequestCompletedFull,
    /// RequestCompletedPartial means the response is completed, and part of the
    /// GraphSync request was sent back, but not the complete request.
    RequestCompletedPartial,

    // Error Response Codes (request terminated)
    /// RequestRejected means the node did not accept the incoming request.
    RequestRejected,
    /// RequestFailedBusy means the node is too busy, try again later. Backoff may
    /// be contained in extra.
    RequestFailedBusy,
    /// RequestFailedUnknown means the request failed for an unspecified reason. May
    /// contain data about why in extra.
    RequestFailedUnknown,
    /// RequestFailedLegal means the request failed for legal reasons.
    RequestFailedLegal,
    /// RequestFailedContentNotFound means the respondent does not have the content.
    RequestFailedContentNotFound,
    Other(i32),
}

impl ResponseStatusCode {
    /// Return the integer responding to the status code
    pub fn to_i32(self) -> i32 {
        match self {
            Self::Other(code) => code,
            Self::RequestAcknowledged => 10,
            Self::AdditionalPeers => 11,
            Self::NotEnoughGas => 12,
            Self::OtherProtocol => 13,
            Self::PartialResponse => 14,
            Self::RequestPaused => 15,
            Self::RequestCompletedFull => 20,
            Self::RequestCompletedPartial => 21,
            Self::RequestRejected => 30,
            Self::RequestFailedBusy => 31,
            Self::RequestFailedUnknown => 32,
            Self::RequestFailedLegal => 33,
            Self::RequestFailedContentNotFound => 34,
        }
    }

    /// Return the status code for a given integer.
    pub fn from_i32(code: i32) -> Self {
        match code {
            10 => Self::RequestAcknowledged,
            11 => Self::AdditionalPeers,
            12 => Self::NotEnoughGas,
            13 => Self::OtherProtocol,
            14 => Self::PartialResponse,
            15 => Self::RequestPaused,
            20 => Self::RequestCompletedFull,
            21 => Self::RequestCompletedPartial,
            30 => Self::RequestRejected,
            31 => Self::RequestFailedBusy,
            32 => Self::RequestFailedUnknown,
            33 => Self::RequestFailedLegal,
            34 => Self::RequestFailedContentNotFound,
            _ => Self::Other(code),
        }
    }
}

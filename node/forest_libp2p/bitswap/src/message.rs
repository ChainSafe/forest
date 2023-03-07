// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use pb::bitswap_pb;
use protobuf::{EnumOrUnknown, Message};
use serde::{Deserialize, Serialize};

use crate::{prefix::Prefix, *};

/// Type of a `bitswap` request
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
pub enum RequestType {
    Have,
    Block,
}

impl From<bitswap_pb::message::wantlist::WantType> for RequestType {
    fn from(value: bitswap_pb::message::wantlist::WantType) -> Self {
        match value {
            bitswap_pb::message::wantlist::WantType::Have => RequestType::Have,
            bitswap_pb::message::wantlist::WantType::Block => RequestType::Block,
        }
    }
}

impl TryFrom<EnumOrUnknown<bitswap_pb::message::wantlist::WantType>> for RequestType {
    type Error = i32;

    fn try_from(
        value: EnumOrUnknown<bitswap_pb::message::wantlist::WantType>,
    ) -> Result<Self, Self::Error> {
        value.enum_value().map(Into::into)
    }
}

impl From<RequestType> for bitswap_pb::message::wantlist::WantType {
    fn from(value: RequestType) -> Self {
        match value {
            RequestType::Have => bitswap_pb::message::wantlist::WantType::Have,
            RequestType::Block => bitswap_pb::message::wantlist::WantType::Block,
        }
    }
}

impl From<RequestType> for EnumOrUnknown<bitswap_pb::message::wantlist::WantType> {
    fn from(value: RequestType) -> Self {
        let want_type: bitswap_pb::message::wantlist::WantType = value.into();
        want_type.into()
    }
}

/// `Bitswap` request type
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct BitswapRequest {
    pub ty: RequestType,
    pub cid: Cid,
    pub send_dont_have: bool,
    pub cancel: bool,
}

impl BitswapRequest {
    pub fn new_have(cid: Cid) -> Self {
        Self {
            ty: RequestType::Have,
            cid,
            send_dont_have: false,
            cancel: false,
        }
    }

    pub fn new_block(cid: Cid) -> Self {
        Self {
            ty: RequestType::Block,
            cid,
            send_dont_have: false,
            cancel: false,
        }
    }

    pub fn send_dont_have(mut self, b: bool) -> Self {
        self.send_dont_have = b;
        self
    }

    pub fn new_cancel(cid: Cid) -> Self {
        // Matches `https://github.com/ipfs/go-libipfs/blob/v0.6.0/bitswap/message/message.go#L309`
        Self {
            ty: RequestType::Block,
            cid,
            send_dont_have: false,
            cancel: true,
        }
    }
}

/// `Bitswap` response type
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum BitswapResponse {
    Have(bool),
    Block(Vec<u8>),
}

/// `Bitswap` message enum type that is either a [BitswapRequest] or a
/// [BitswapResponse]
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum BitswapMessage {
    Request(BitswapRequest),
    Response(Cid, BitswapResponse),
}

impl BitswapMessage {
    pub fn to_bytes(&self) -> IOResult<Vec<u8>> {
        let mut msg = bitswap_pb::Message::new();
        match self {
            Self::Request(BitswapRequest {
                ty,
                cid,
                send_dont_have,
                cancel,
            }) => {
                let mut wantlist = bitswap_pb::message::Wantlist::new();

                wantlist.entries.push({
                    let mut entry = bitswap_pb::message::wantlist::Entry::new();
                    entry.block = cid.to_bytes();
                    entry.wantType = (*ty).into();
                    entry.sendDontHave = *send_dont_have;
                    entry.cancel = *cancel;
                    entry.priority = 1;
                    entry
                });

                msg.wantlist = Some(wantlist).into();
            }
            Self::Response(cid, BitswapResponse::Have(have)) => {
                let mut block_presence = bitswap_pb::message::BlockPresence::new();

                block_presence.cid = cid.to_bytes();
                block_presence.type_ = if *have {
                    bitswap_pb::message::BlockPresenceType::Have
                } else {
                    bitswap_pb::message::BlockPresenceType::DontHave
                }
                .into();

                msg.blockPresences.push(block_presence);
            }
            Self::Response(cid, BitswapResponse::Block(bytes)) => {
                let mut payload = bitswap_pb::message::Block::new();

                payload.prefix = Prefix::from(cid).to_bytes();
                payload.data = bytes.to_vec();

                msg.payload.push(payload);
            }
        }
        msg.write_to_bytes().map_err(map_io_err)
    }
}

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{prefix::Prefix, *};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestType {
    Have,
    Block,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BitswapRequest {
    pub ty: RequestType,
    pub cid: Cid,
    pub send_dont_have: bool,
}

impl BitswapRequest {
    pub fn new_have(cid: Cid) -> Self {
        Self {
            ty: RequestType::Have,
            cid,
            send_dont_have: false,
        }
    }

    pub fn new_block(cid: Cid) -> Self {
        Self {
            ty: RequestType::Block,
            cid,
            send_dont_have: false,
        }
    }

    pub fn send_dont_have(mut self, b: bool) -> Self {
        self.send_dont_have = b;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BitswapResponse {
    Have(bool),
    Block(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BitswapMessage {
    Request(BitswapRequest),
    Response(Cid, BitswapResponse),
}

impl BitswapMessage {
    pub fn to_bytes(&self) -> IOResult<Vec<u8>> {
        let mut msg = proto::Message::default();
        match self {
            Self::Request(BitswapRequest {
                ty,
                cid,
                send_dont_have,
            }) => {
                let mut wantlist = proto::message::Wantlist::default();
                let entry = proto::message::wantlist::Entry {
                    block: cid.to_bytes(),
                    want_type: match ty {
                        RequestType::Have => proto::message::wantlist::WantType::Have,
                        RequestType::Block => proto::message::wantlist::WantType::Block,
                    } as _,
                    send_dont_have: *send_dont_have,
                    cancel: false,
                    priority: 1,
                };
                wantlist.entries.push(entry);
                msg.wantlist = Some(wantlist);
            }
            Self::Response(cid, BitswapResponse::Have(have)) => {
                let block_presence = proto::message::BlockPresence {
                    cid: cid.to_bytes(),
                    r#type: if *have {
                        proto::message::BlockPresenceType::Have
                    } else {
                        proto::message::BlockPresenceType::DontHave
                    } as _,
                };
                msg.block_presences.push(block_presence);
            }
            Self::Response(cid, BitswapResponse::Block(bytes)) => {
                let payload = proto::message::Block {
                    prefix: Prefix::from(cid).to_bytes(),
                    data: bytes.to_vec(),
                };
                msg.payload.push(payload);
            }
        }
        let mut bytes = Vec::with_capacity(msg.encoded_len());
        msg.encode(&mut bytes).map_err(map_io_err)?;
        Ok(bytes)
    }
}

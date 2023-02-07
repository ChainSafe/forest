// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use libp2p::{core::upgrade, request_response::RequestResponseCodec};
use pb::bitswap_pb;
use protobuf::Message;

use crate::{prefix::Prefix, protocol::*, *};

// 2MB Block Size according to the specs at https://github.com/ipfs/specs/blob/main/BITSWAP.md
const MAX_BUF_SIZE: usize = 1024 * 1024 * 2;

#[derive(Debug, Clone)]
pub struct BitswapRequestResponseCodec;

#[async_trait]
impl RequestResponseCodec for BitswapRequestResponseCodec {
    type Protocol = BitswapProtocol;
    type Request = Vec<BitswapMessage>;
    type Response = ();

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> IOResult<Self::Request>
    where
        T: AsyncRead + Send + Unpin,
    {
        let data = upgrade::read_length_prefixed(io, MAX_BUF_SIZE).await?;

        metrics::inbound_stream_count().inc();
        metrics::inbound_bytes().inc_by(data.len() as _);

        let pb_msg = bitswap_pb::Message::parse_from_bytes(data.as_slice()).map_err(map_io_err)?;
        let mut parts = vec![];
        for entry in pb_msg.wantlist.unwrap_or_default().entries {
            // TODO: Implement cancellation
            if entry.cancel {
                continue;
            }
            let cid = Cid::try_from(entry.block).map_err(map_io_err)?;
            let ty = match entry.wantType.try_into() {
                Ok(ty) => ty,
                Err(e) => {
                    tracing::error!("Skipping invalid request type: {e}");
                    continue;
                }
            };
            parts.push(BitswapMessage::Request(BitswapRequest {
                ty,
                cid,
                send_dont_have: entry.sendDontHave,
            }));
        }
        for payload in pb_msg.payload {
            let prefix = Prefix::new(&payload.prefix).map_err(map_io_err)?;
            let cid = prefix.to_cid(&payload.data).map_err(map_io_err)?;
            parts.push(BitswapMessage::Response(
                cid,
                BitswapResponse::Block(payload.data.to_vec()),
            ));
        }
        for presence in pb_msg.blockPresences {
            let cid = Cid::try_from(presence.cid).map_err(map_io_err)?;
            let have = match presence.type_.enum_value() {
                Ok(bitswap_pb::message::BlockPresenceType::Have) => true,
                Ok(bitswap_pb::message::BlockPresenceType::DontHave) => false,
                Err(e) => {
                    error!("Skipping invalid block presence type {e}");
                    continue;
                }
            };
            parts.push(BitswapMessage::Response(cid, BitswapResponse::Have(have)));
        }
        Ok(parts)
    }

    /// Just close the outbound stream,
    /// the actual responses will come from new inbound stream
    /// and be received in `read_request`
    async fn read_response<T>(&mut self, _: &Self::Protocol, _: &mut T) -> IOResult<Self::Response>
    where
        T: AsyncRead + Send + Unpin,
    {
        Ok(())
    }

    /// Sending both `bitswap` requests and responses
    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        messages: Self::Request,
    ) -> IOResult<()>
    where
        T: AsyncWrite + Send + Unpin,
    {
        // TODO: Low priority, batch sending is not supported in `libp2p-bitswap` either
        // panic here means bug in public API of this crate
        assert!(
            messages.len() == 1,
            "It's only supported to send a single message"
        );

        let bytes = messages[0].to_bytes()?;

        metrics::outbound_stream_count().inc();
        metrics::outbound_bytes().inc_by(bytes.len() as _);

        upgrade::write_length_prefixed(io, bytes).await
    }

    // Sending `FIN` header and close the stream
    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        _: &mut T,
        _: Self::Response,
    ) -> IOResult<()>
    where
        T: AsyncWrite + Send + Unpin,
    {
        Ok(())
    }
}

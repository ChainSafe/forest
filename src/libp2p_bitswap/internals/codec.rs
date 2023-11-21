// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io;

use async_trait::async_trait;
use asynchronous_codec::{FramedRead, FramedWrite};
use futures::{
    io::{AsyncRead, AsyncWrite},
    SinkExt, StreamExt,
};
use libp2p::request_response;

use crate::libp2p_bitswap::{bitswap_pb::mod_Message::BlockPresenceType, prefix::Prefix, *};

// 2MB Block Size according to the specs at https://github.com/ipfs/specs/blob/main/BITSWAP.md
const MAX_BUF_SIZE: usize = 1024 * 1024 * 2;

fn codec() -> quick_protobuf_codec::Codec<bitswap_pb::Message> {
    quick_protobuf_codec::Codec::<bitswap_pb::Message>::new(MAX_BUF_SIZE)
}

#[derive(Default, Debug, Clone)]
pub struct BitswapRequestResponseCodec;

#[async_trait]
impl request_response::Codec for BitswapRequestResponseCodec {
    type Protocol = &'static str;
    type Request = Vec<BitswapMessage>;
    type Response = ();

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> IOResult<Self::Request>
    where
        T: AsyncRead + Send + Unpin,
    {
        let pb_msg: bitswap_pb::Message = FramedRead::new(io, codec())
            .next()
            .await
            .ok_or(std::io::ErrorKind::UnexpectedEof)??;

        metrics::inbound_stream_count().inc();

        let mut parts = vec![];
        for entry in pb_msg.wantlist.unwrap_or_default().entries {
            let cid = Cid::try_from(entry.block).map_err(io::Error::other)?;
            parts.push(BitswapMessage::Request(BitswapRequest {
                ty: entry.wantType.into(),
                cid,
                send_dont_have: entry.sendDontHave,
                cancel: entry.cancel,
            }));
        }

        for payload in pb_msg.payload {
            let prefix = Prefix::new(&payload.prefix).map_err(io::Error::other)?;
            let cid = prefix.to_cid(&payload.data).map_err(io::Error::other)?;
            parts.push(BitswapMessage::Response(
                cid,
                BitswapResponse::Block(payload.data.to_vec()),
            ));
        }

        for presence in pb_msg.blockPresences {
            let cid = Cid::try_from(presence.cid).map_err(io::Error::other)?;
            let have = presence.type_pb == BlockPresenceType::Have;
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
        mut messages: Self::Request,
    ) -> IOResult<()>
    where
        T: AsyncWrite + Send + Unpin,
    {
        assert_eq!(
            messages.len(),
            1,
            "It's only supported to send a single message" // libp2p-bitswap doesn't support batch sending
        );

        let data = messages.swap_remove(0).into_proto()?;
        let mut framed = FramedWrite::new(io, codec());
        framed.send(data).await?;
        framed.close().await?;

        metrics::outbound_stream_count().inc();

        Ok(())
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

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io;

use async_trait::async_trait;
use libp2p::request_response;
use pb::bitswap_pb;
use protobuf::Message;

use crate::libp2p_bitswap::{prefix::Prefix, *};

// 2MB Block Size according to the specs at https://github.com/ipfs/specs/blob/main/BITSWAP.md
const MAX_BUF_SIZE: usize = 1024 * 1024 * 2;

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
        let data = read_length_prefixed(io, MAX_BUF_SIZE).await?;

        metrics::inbound_stream_count().inc();
        metrics::inbound_bytes().inc_by(data.len() as _);

        let pb_msg = bitswap_pb::Message::parse_from_bytes(data.as_slice()).map_err(map_io_err)?;
        let mut parts = vec![];
        for entry in pb_msg.wantlist.unwrap_or_default().entries {
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
                cancel: entry.cancel,
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
        assert_eq!(
            messages.len(),
            1,
            "It's only supported to send a single message" // libp2p-bitswap doesn't support batch sending
        );

        let bytes = messages[0].to_bytes()?;

        metrics::outbound_stream_count().inc();
        metrics::outbound_bytes().inc_by(bytes.len() as _);

        write_length_prefixed(io, bytes).await
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

/// Ported from <https://github.com/libp2p/rust-libp2p/commit/fab920500ddee04be65a4108a1abdb2e91a30b1b#diff-2b9bbd8d3a6c42b5b470dd4e5ec54780a7a43dd39cf8735baed752289696df9dL100>
/// Reads a length-prefixed message from the given socket.
///
/// The `max_size` parameter is the maximum size in bytes of the message that we accept. This is
/// necessary in order to avoid `DoS` attacks where the remote sends us a message of several
/// gigabytes.
///
/// > **Note**: Assumes that a variable-length prefix indicates the length of the message. This is
/// >           compatible with what [`write_length_prefixed`] does.
async fn read_length_prefixed(
    socket: &mut (impl AsyncRead + Unpin),
    max_size: usize,
) -> io::Result<Vec<u8>> {
    let len = read_varint(socket).await?;
    if len > max_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Received data size ({len} bytes) exceeds maximum ({max_size} bytes)"),
        ));
    }

    let mut buf = vec![0; len];
    socket.read_exact(&mut buf).await?;

    Ok(buf)
}

/// Ported from <https://github.com/libp2p/rust-libp2p/commit/fab920500ddee04be65a4108a1abdb2e91a30b1b#diff-2b9bbd8d3a6c42b5b470dd4e5ec54780a7a43dd39cf8735baed752289696df9dL28>
/// Writes a message to the given socket with a length prefix appended to it. Also flushes the socket.
///
/// > **Note**: Prepend a variable-length prefix indicate the length of the message. This is
/// >           compatible with what [`read_length_prefixed`] expects.
async fn write_length_prefixed(
    socket: &mut (impl AsyncWrite + Unpin),
    data: impl AsRef<[u8]>,
) -> Result<(), io::Error> {
    write_varint(socket, data.as_ref().len()).await?;
    socket.write_all(data.as_ref()).await?;
    socket.flush().await?;

    Ok(())
}

/// Ported from <https://github.com/libp2p/rust-libp2p/commit/fab920500ddee04be65a4108a1abdb2e91a30b1b#diff-2b9bbd8d3a6c42b5b470dd4e5ec54780a7a43dd39cf8735baed752289696df9dL64>
/// Reads a variable-length integer from the `socket`.
///
/// As a special exception, if the `socket` is empty and `EOF`s right at the beginning, then we
/// return `Ok(0)`.
///
/// > **Note**: This function reads bytes one by one from the `socket`. It is therefore encouraged
/// >           to use some sort of buffering mechanism.
async fn read_varint(socket: &mut (impl AsyncRead + Unpin)) -> io::Result<usize> {
    let mut buffer = unsigned_varint::encode::usize_buffer();
    let mut buffer_len = 0;

    loop {
        match socket.read(&mut buffer[buffer_len..buffer_len + 1]).await? {
            0 => {
                // Reaching EOF before finishing to read the length is an error, unless the EOF is
                // at the very beginning of the substream, in which case we assume that the data is
                // empty.
                if buffer_len == 0 {
                    return Ok(0);
                } else {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
            }
            n => debug_assert_eq!(n, 1),
        }

        buffer_len += 1;

        match unsigned_varint::decode::usize(&buffer[..buffer_len]) {
            Ok((len, _)) => return Ok(len),
            Err(unsigned_varint::decode::Error::Overflow) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "overflow in variable-length integer",
                ));
            }
            // TODO: why do we have a `__Nonexhaustive` variant in the error? I don't know how to process it
            // Err(unsigned_varint::decode::Error::Insufficient) => {}
            Err(_) => {}
        }
    }
}

/// Ported from <https://github.com/libp2p/rust-libp2p/commit/fab920500ddee04be65a4108a1abdb2e91a30b1b#diff-2b9bbd8d3a6c42b5b470dd4e5ec54780a7a43dd39cf8735baed752289696df9dL43>
/// Writes a variable-length integer to the `socket`.
///
/// > **Note**: Does **NOT** flush the socket.
pub async fn write_varint(
    socket: &mut (impl AsyncWrite + Unpin),
    len: usize,
) -> Result<(), io::Error> {
    let mut len_data = unsigned_varint::encode::usize_buffer();
    let encoded_len = unsigned_varint::encode::usize(len, &mut len_data).len();
    socket.write_all(&len_data[..encoded_len]).await?;

    Ok(())
}

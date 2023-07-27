// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use bytes::{Buf, BufMut, Bytes, BytesMut};
use integer_encoding::VarInt;
pub use serde::{de, ser, Deserializer, Serializer};
use std::io;
use tokio_util::codec::{Decoder, Encoder};

// Unsigned VarInt (Uvi) Bytes
pub struct UviBytes {
    // cache for varint frame size
    len: Option<usize>,
    // content size limit, defaults to 128MiB
    limit: usize,
}

impl UviBytes {
    pub fn new_with_limit(limit: usize) -> Self {
        UviBytes { len: None, limit }
    }
}

impl Default for UviBytes {
    fn default() -> Self {
        UviBytes::new_with_limit(128 * 1024 * 1024)
    }
}

impl Decoder for UviBytes {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.len.is_none() {
            if let Some((n, size)) = usize::decode_var(src) {
                src.advance(size);
                self.len = Some(n);
            } else if src.len() >= std::mem::size_of::<usize>() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid uvi frame",
                ));
            }
        }
        if let Some(n) = self.len.take() {
            if n > self.limit {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("uvi frame size limit exceeded, decode {}", n),
                ));
            }
            if n <= src.len() {
                return Ok(Some(src.split_to(n)));
            }
            src.reserve(n - src.len());
            self.len = Some(n);
        }
        Ok(None)
    }
}

impl Encoder<Bytes> for UviBytes {
    type Error = io::Error;

    fn encode(&mut self, item: Bytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if item.remaining() > self.limit {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("uvi frame size limit exceeded, encode {}", item.remaining()),
            ));
        }
        dst.reserve(item.remaining() + item.remaining().required_space());
        let mut buffer = [0; 16];
        let len = item.remaining().encode_var(&mut buffer);
        dst.extend_from_slice(&buffer[0..len]);
        dst.put(item);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::*;

    #[test]
    fn uvibytes_encode_size_limit() {
        assert!(UviBytes::new_with_limit(10)
            .encode(Bytes::from_static(&[0; 1024]), &mut BytesMut::new())
            .is_err());
    }

    #[test]
    fn uvibytes_decode_size_limit() {
        let mut bytes = BytesMut::new();
        UviBytes::default()
            .encode(Bytes::from_static(&[0; 1024]), &mut bytes)
            .unwrap();
        assert!(UviBytes::new_with_limit(10).decode(&mut bytes).is_err());
    }

    #[test]
    fn uvibytes_partial_decode() {
        let mut bytes: BytesMut = BytesMut::from(&[u8::MAX][0..]);
        assert_eq!(
            UviBytes::new_with_limit(10).decode(&mut bytes).unwrap(),
            None
        );
    }

    #[quickcheck]
    fn uvibytes_roundtrip(data: Vec<u8>) {
        let mut buffer = BytesMut::new();
        UviBytes::default()
            .encode(Bytes::from(data.clone()), &mut buffer)
            .unwrap();
        let out = UviBytes::default().decode(&mut buffer).unwrap().unwrap();
        assert_eq!(data, out.to_vec());
    }
}

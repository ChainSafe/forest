// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Automatically generated rust module for 'drand_pb.proto' file
// Command: `pb-rs -s -D proto/drand_pb.proto`, See <https://crates.io/crates/pb-rs>

#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(unused_imports)]
#![allow(unknown_lints)]
#![allow(clippy::all)]
#![cfg_attr(rustfmt, rustfmt_skip)]


use quick_protobuf::{MessageInfo, MessageRead, MessageWrite, BytesReader, Writer, WriterBackend, Result};
use quick_protobuf::sizeofs::*;
use super::*;

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct PublicRandResponse {
    pub round: u64,
    pub signature: Vec<u8>,
}

impl<'a> MessageRead<'a> for PublicRandResponse {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.round = r.read_uint64(bytes)?,
                Ok(18) => msg.signature = r.read_bytes(bytes)?.to_owned(),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for PublicRandResponse {
    fn get_size(&self) -> usize {
        0
        + if self.round == 0u64 { 0 } else { 1 + sizeof_varint(*(&self.round) as u64) }
        + if self.signature.is_empty() { 0 } else { 1 + sizeof_len((&self.signature).len()) }
    }

    fn write_message<W: WriterBackend>(&self, w: &mut Writer<W>) -> Result<()> {
        if self.round != 0u64 { w.write_with_tag(8, |w| w.write_uint64(*&self.round))?; }
        if !self.signature.is_empty() { w.write_with_tag(18, |w| w.write_bytes(&**&self.signature))?; }
        Ok(())
    }
}


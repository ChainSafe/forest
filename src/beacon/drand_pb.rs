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
#![allow(clippy::assigning_clones)]
#![allow(clippy::cast_lossless)]
#![cfg_attr(rustfmt, rustfmt_skip)]


use quick_protobuf::{MessageInfo, MessageRead, MessageWrite, BytesReader, Writer, WriterBackend, Result};
use quick_protobuf::sizeofs::*;
use super::*;

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct PublicRandResponse {
    pub round: u64,
    pub signature: Vec<u8>,
    pub previous_signature: Vec<u8>,
    pub metadata: Option<Metadata>,
}

impl<'a> MessageRead<'a> for PublicRandResponse {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.round = r.read_uint64(bytes)?,
                Ok(18) => msg.signature = r.read_bytes(bytes)?.to_owned(),
                Ok(26) => msg.previous_signature = r.read_bytes(bytes)?.to_owned(),
                Ok(42) => msg.metadata = Some(r.read_message::<Metadata>(bytes)?),
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
        + if self.previous_signature.is_empty() { 0 } else { 1 + sizeof_len((&self.previous_signature).len()) }
        + self.metadata.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
    }

    fn write_message<W: WriterBackend>(&self, w: &mut Writer<W>) -> Result<()> {
        if self.round != 0u64 { w.write_with_tag(8, |w| w.write_uint64(*&self.round))?; }
        if !self.signature.is_empty() { w.write_with_tag(18, |w| w.write_bytes(&**&self.signature))?; }
        if !self.previous_signature.is_empty() { w.write_with_tag(26, |w| w.write_bytes(&**&self.previous_signature))?; }
        if let Some(ref s) = self.metadata { w.write_with_tag(42, |w| w.write_message(s))?; }
        Ok(())
    }
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Metadata {
    pub node_version: Option<NodeVersion>,
    pub beaconID: String,
    pub chain_hash: Vec<u8>,
}

impl<'a> MessageRead<'a> for Metadata {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.node_version = Some(r.read_message::<NodeVersion>(bytes)?),
                Ok(18) => msg.beaconID = r.read_string(bytes)?.to_owned(),
                Ok(26) => msg.chain_hash = r.read_bytes(bytes)?.to_owned(),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for Metadata {
    fn get_size(&self) -> usize {
        0
        + self.node_version.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
        + if self.beaconID == String::default() { 0 } else { 1 + sizeof_len((&self.beaconID).len()) }
        + if self.chain_hash.is_empty() { 0 } else { 1 + sizeof_len((&self.chain_hash).len()) }
    }

    fn write_message<W: WriterBackend>(&self, w: &mut Writer<W>) -> Result<()> {
        if let Some(ref s) = self.node_version { w.write_with_tag(10, |w| w.write_message(s))?; }
        if self.beaconID != String::default() { w.write_with_tag(18, |w| w.write_string(&**&self.beaconID))?; }
        if !self.chain_hash.is_empty() { w.write_with_tag(26, |w| w.write_bytes(&**&self.chain_hash))?; }
        Ok(())
    }
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct NodeVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: String,
}

impl<'a> MessageRead<'a> for NodeVersion {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.major = r.read_uint32(bytes)?,
                Ok(16) => msg.minor = r.read_uint32(bytes)?,
                Ok(24) => msg.patch = r.read_uint32(bytes)?,
                Ok(34) => msg.prerelease = r.read_string(bytes)?.to_owned(),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for NodeVersion {
    fn get_size(&self) -> usize {
        0
        + if self.major == 0u32 { 0 } else { 1 + sizeof_varint(*(&self.major) as u64) }
        + if self.minor == 0u32 { 0 } else { 1 + sizeof_varint(*(&self.minor) as u64) }
        + if self.patch == 0u32 { 0 } else { 1 + sizeof_varint(*(&self.patch) as u64) }
        + if self.prerelease == String::default() { 0 } else { 1 + sizeof_len((&self.prerelease).len()) }
    }

    fn write_message<W: WriterBackend>(&self, w: &mut Writer<W>) -> Result<()> {
        if self.major != 0u32 { w.write_with_tag(8, |w| w.write_uint32(*&self.major))?; }
        if self.minor != 0u32 { w.write_with_tag(16, |w| w.write_uint32(*&self.minor))?; }
        if self.patch != 0u32 { w.write_with_tag(24, |w| w.write_uint32(*&self.patch))?; }
        if self.prerelease != String::default() { w.write_with_tag(34, |w| w.write_string(&**&self.prerelease))?; }
        Ok(())
    }
}


// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::error::Error;
use cid::Cid;
use std::io::Read;

pub(crate) fn ld_read<R: Read>(mut buf_reader: &mut R) -> Result<Vec<u8>, Error> {
    let l =
        unsigned_varint::io::read_u64(&mut buf_reader).map_err(|e| Error::Other(e.to_string()))?;
    let mut buf = Vec::with_capacity(l as usize);
    buf_reader
        .take(l)
        .read_to_end(&mut buf)
        .map_err(|e| Error::Other(e.to_string()))?;
    Ok(buf)
}

pub(crate) fn read_node<R: Read>(buf_reader: &mut R) -> Result<(Cid, Vec<u8>), Error> {
    let buf = ld_read(buf_reader)?;
    let (c, n) = read_cid(&buf)?;
    Ok((c, buf[(n as usize)..].to_owned()))
}

pub(crate) fn read_cid(buf: &[u8]) -> Result<(Cid, u64), Error> {
    // TODO: Add some checks for cid v0
    // TODO: Upgrade the Cid crate to read_cid using a BufReader
    let (version, buf) =
        unsigned_varint::decode::u64(buf).map_err(|e| Error::ParsingError(e.to_string()))?;
    let (codec, multihash_with_data) =
        unsigned_varint::decode::u64(buf).map_err(|e| Error::ParsingError(e.to_string()))?;
    // multihash part
    let (_hashcode, buf) = unsigned_varint::decode::u64(multihash_with_data)
        .map_err(|e| Error::ParsingError(e.to_string()))?;
    let hashcode_len_diff = multihash_with_data.len() - buf.len();
    let (len, _) =
        unsigned_varint::decode::u64(buf).map_err(|e| Error::ParsingError(e.to_string()))?;

    let cid: Cid = Cid::new(
        cid::Codec::from(codec)?,
        cid::Version::from(version)?,
        cid::multihash::Multihash::from_bytes(
            multihash_with_data[0..=(len as usize + hashcode_len_diff)].to_vec(),
        )?,
    );
    let len = cid.to_bytes().len() as u64;
    Ok((cid, len))
}

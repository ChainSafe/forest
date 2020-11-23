// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::error::Error;
use cid::Cid;
use std::io::Read;
use unsigned_varint::io::ReadError;

pub(crate) fn ld_read<R: Read>(mut reader: &mut R) -> Result<Option<Vec<u8>>, Error> {
    let l = match unsigned_varint::io::read_u64(&mut reader) {
        Ok(len) => len,
        Err(e) => {
            if let ReadError::Io(ioe) = &e {
                if ioe.kind() == std::io::ErrorKind::UnexpectedEof {
                    return Ok(None);
                }
            }
            return Err(Error::Other(e.to_string()));
        }
    };
    let mut buf = Vec::with_capacity(l as usize);
    reader
        .take(l)
        .read_to_end(&mut buf)
        .map_err(|e| Error::Other(e.to_string()))?;
    Ok(Some(buf))
}

pub(crate) fn read_node<R: Read>(buf_reader: &mut R) -> Result<Option<(Cid, Vec<u8>)>, Error> {
    match ld_read(buf_reader)? {
        Some(buf) => {
            let cid = Cid::read_bytes(&*buf)?;
            let len = cid.to_bytes().len();
            Ok(Some((cid, buf[(len as usize)..].to_owned())))
        }
        None => Ok(None),
    }
}

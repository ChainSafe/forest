// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::error::Error;
use cid::Cid;
use futures::{AsyncWrite, AsyncWriteExt};
use integer_encoding::{VarIntAsyncWriter, VarIntReader};
use std::io::Read;

pub(crate) fn ld_read<R: Read>(mut reader: &mut R) -> Result<Option<Vec<u8>>, Error> {
    let l: usize = match VarIntReader::read_varint(&mut reader) {
        Ok(len) => len,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(Error::Other(e.to_string()));
        }
    };
    let mut buf = Vec::with_capacity(l as usize);
    reader
        .take(l as u64)
        .read_to_end(&mut buf)
        .map_err(|e| Error::Other(e.to_string()))?;
    Ok(Some(buf))
}

pub(crate) async fn ld_write<'a, W>(writer: &mut W, bytes: &[u8]) -> Result<(), Error>
where
    W: AsyncWrite + Send + Unpin,
{
    writer.write_varint_async(bytes.len()).await?;
    writer.write_all(bytes).await?;
    writer.flush().await?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[async_std::test]
    async fn ld_read_write() {
        let mut buffer = Vec::<u8>::new();
        ld_write(&mut buffer, b"test bytes").await.unwrap();
        let mut reader = Cursor::new(&buffer);
        let read = ld_read(&mut reader).unwrap();
        assert_eq!(read, Some(b"test bytes".to_vec()));
    }
}

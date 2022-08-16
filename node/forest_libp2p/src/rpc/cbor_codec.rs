// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Code originally from futures_cbor_codec. License: Apache-2.0/MIT

use {
    asynchronous_codec::Decoder as IoDecoder,
    bytes::{Buf, BytesMut},
    serde::Deserialize,
    serde_ipld_dagcbor::{
        de::{Deserializer, IoRead},
        Error as CborError,
    },
    std::{
        error::Error as ErrorTrait,
        fmt::{Display, Formatter, Result as FmtResult},
        io::{Error as IoError, Read, Result as IoResult},
        marker::PhantomData,
    },
};

//
/// Errors returned by encoding and decoding.
//
#[derive(Debug)]
#[non_exhaustive]
//
pub enum Error {
    /// An io error happened on the underlying stream.
    //
    Io(IoError),

    /// An error happend when encoding/decoding Cbor data.
    //
    Cbor(CborError),
}

impl From<IoError> for Error {
    fn from(error: IoError) -> Self {
        Error::Io(error)
    }
}

impl From<CborError> for Error {
    fn from(error: CborError) -> Self {
        Error::Cbor(error)
    }
}

impl Display for Error {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        match self {
            Error::Io(e) => e.fmt(fmt),
            Error::Cbor(e) => e.fmt(fmt),
        }
    }
}

impl ErrorTrait for Error {
    fn cause(&self) -> Option<&dyn ErrorTrait> {
        match self {
            Error::Io(e) => Some(e),
            Error::Cbor(e) => Some(e),
        }
    }
}

/// A `Read` wrapper that also counts the used bytes.
///
/// This wraps a `Read` into another `Read` that keeps track of how many bytes were read. This is
/// needed, as there's no way to get the position out of the CBOR decoder.
//
struct Counted<'a, R> {
    r: &'a mut R,
    pos: &'a mut usize,
}

impl<R: Read> Read for Counted<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        match self.r.read(buf) {
            Ok(size) => {
                *self.pos += size;
                Ok(size)
            }
            e => e,
        }
    }
}

/// CBOR based decoder.
///
/// This decoder can be used with `future_codec`'s `FramedRead` to decode CBOR encoded frames. Anything
/// that is `serde`s `Deserialize` can be decoded this way.
//
#[derive(Clone, Debug)]
//
pub struct Decoder<Item> {
    _data: PhantomData<fn() -> Item>,
}

impl<'de, Item: Deserialize<'de>> Decoder<Item> {
    /// Creates a new decoder.
    //
    pub fn new() -> Self {
        Self { _data: PhantomData }
    }
}

impl<'de, Item: Deserialize<'de>> Default for Decoder<Item> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de, Item: Deserialize<'de>> IoDecoder for Decoder<Item> {
    type Item = Item;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Item>, Error> {
        let mut pos = 0;
        let mut slice: &[u8] = src;

        let reader = IoRead::new(Counted {
            r: &mut slice,
            pos: &mut pos,
        });

        // Use the deserializer directly, instead of using `deserialize_from`. We explicitly do
        // *not* want to check that there are no trailing bytes ‒ there may be, and they are
        // the next frame.
        //
        let mut deserializer = Deserializer::new(reader);

        match Item::deserialize(&mut deserializer) {
            // If we read the item, we also need to consume the corresponding bytes.
            //
            Ok(item) => {
                src.advance(pos);
                Ok(Some(item))
            }

            // Sometimes the EOF is signalled as IO error
            //
            Err(ref error) if error.is_eof() => Ok(None),

            // Any other error is simply passed through.
            //
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    type TestData = HashMap<String, usize>;

    /// Something to test with. It doesn't really matter what it is.
    fn test_data() -> TestData {
        let mut data = HashMap::new();
        data.insert("hello".to_owned(), 42usize);
        data.insert("world".to_owned(), 0usize);
        data
    }

    /// Try decoding CBOR based data.
    fn decode<Dec: IoDecoder<Item = TestData, Error = Error>>(dec: Dec) {
        let mut decoder = dec;
        let data = test_data();
        let encoded = serde_ipld_dagcbor::to_vec(&data).unwrap();
        let mut all = BytesMut::with_capacity(128);
        // Put two copies and a bit into the buffer
        all.extend(&encoded);
        all.extend(&encoded);
        all.extend(&encoded[..1]);
        // We can now decode the first two copies
        let decoded = decoder.decode(&mut all).unwrap().unwrap();
        assert_eq!(data, decoded);
        let decoded = decoder.decode(&mut all).unwrap().unwrap();
        assert_eq!(data, decoded);
        // And only 1 byte is left
        assert_eq!(1, all.len());
        // But the third one is not ready yet, so we get Ok(None)
        assert!(decoder.decode(&mut all).unwrap().is_none());
        // That single byte should still be there, yet unused
        assert_eq!(1, all.len());
        // We add the rest and get a third copy
        all.extend(&encoded[1..]);
        let decoded = decoder.decode(&mut all).unwrap().unwrap();
        assert_eq!(data, decoded);
        // Nothing there now
        assert!(all.is_empty());
        // Now we put some garbage there and see that it errors
        all.extend(&[0, 1, 2, 3, 4]);
        decoder.decode(&mut all).unwrap_err();
        // All 5 bytes are still there
        assert_eq!(5, all.len());
    }

    /// Run the decoding tests on the lone decoder.
    #[test]
    fn decode_only() {
        let decoder = Decoder::new();
        decode(decoder);
    }
}

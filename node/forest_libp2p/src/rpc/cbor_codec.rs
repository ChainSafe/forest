// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Code originally from futures_cbor_codec. License: Apache-2.0/MIT

use asynchronous_codec::Decoder as IoDecoder;
use bytes::{Buf, BytesMut};
use forest_encoding::{de::DeserializeOwned, error::*};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    io::{BufReader, Error as IoError},
    marker::PhantomData,
};

//
/// Errors returned by encoding and decoding.
//
#[derive(Debug)]
#[non_exhaustive]
//
pub enum Error {
    /// An IO error happened on the underlying stream.
    //
    Io(IoError),

    /// An error happened when encoding/decoding `Cbor` data.
    //
    Cbor(anyhow::Error),
}

impl From<IoError> for Error {
    fn from(error: IoError) -> Self {
        Error::Io(error)
    }
}

impl<E: std::fmt::Debug> From<CborEncodeError<E>> for Error {
    fn from(e: CborEncodeError<E>) -> Self {
        Error::Cbor(anyhow::Error::msg(format!("{e:?}")))
    }
}

impl<E: std::fmt::Debug> From<CborDecodeError<E>> for Error {
    fn from(e: CborDecodeError<E>) -> Self {
        Error::Cbor(anyhow::Error::msg(format!("{e:?}")))
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

impl<Item: Serialize + DeserializeOwned> IoDecoder for Decoder<Item> {
    type Item = Item;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Item>, Error> {
        // let mut pos = 0;
        let slice: &[u8] = src;
        let reader = BufReader::new(slice);

        // Use the deserializer directly, instead of using `deserialize_from`. We explicitly do
        // *not* want to check that there are no trailing bytes ‒ there may be, and they are
        // the next frame.
        //
        // let mut deserializer = Deserializer::new(reader);
        match serde_ipld_dagcbor::de::from_reader_unstrict(reader) {
            // If we read the item, we also need to consume the corresponding bytes.
            Ok(item) => {
                let offset = fvm_ipld_encoding::to_vec(&item).expect("Infallible").len();
                src.advance(offset);
                Ok(Some(item))
            }
            // Sometimes the EOF is signalled as IO error
            Err(CborDecodeError::Eof) => Ok(None),
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
    fn test_data1() -> TestData {
        let mut data = HashMap::new();
        data.insert("hello".to_owned(), 42usize);
        data.insert("world".to_owned(), 0usize);
        data
    }

    /// Something to test with. It doesn't really matter what it is.
    fn test_data2() -> TestData {
        let mut data = HashMap::new();
        data.insert("hello".to_owned(), 33usize);
        data.insert("forest".to_owned(), 22usize);
        data
    }

    /// Try decoding CBOR based data.
    fn decode<Dec: IoDecoder<Item = TestData, Error = Error>>(dec: Dec) {
        let mut decoder = dec;
        let data1 = test_data1();
        let encoded1 = serde_ipld_dagcbor::to_vec(&data1).unwrap();
        let data2 = test_data2();
        let encoded2 = serde_ipld_dagcbor::to_vec(&data2).unwrap();
        let mut all = BytesMut::with_capacity(128);
        // Put two copies and a bit into the buffer
        all.extend(&encoded1);
        all.extend(&encoded2);
        all.extend(&encoded1[..1]);
        // We can now decode the first two copies
        let decoded = decoder.decode(&mut all).unwrap().unwrap();
        assert_eq!(data1, decoded);
        let decoded = decoder.decode(&mut all).unwrap().unwrap();
        assert_eq!(data2, decoded);
        // And only 1 byte is left
        assert_eq!(1, all.len());
        // But the third one is not ready yet, so we get Ok(None)
        assert!(decoder.decode(&mut all).unwrap().is_none());
        // That single byte should still be there, yet unused
        assert_eq!(1, all.len());
        // We add the rest and get a third copy
        all.extend(&encoded1[1..]);
        let decoded = decoder.decode(&mut all).unwrap().unwrap();
        assert_eq!(data1, decoded);
        // Nothing there now
        assert!(all.is_empty());
        // These bytes can be deserialized now
        // // Now we put some garbage there and see that it errors
        // all.extend([0, 1, 2, 3, 4, 5]);
        // decoder.decode(&mut all).unwrap_err();
        // // All 5 bytes are still there
        // assert_eq!(5, all.len());
    }

    /// Run the decoding tests on the lone decoder.
    #[test]
    fn decode_only() {
        let decoder = Decoder::new();
        decode(decoder);
    }
}

// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Code originally from futures_cbor_codec. License: Apache-2.0/MIT

use asynchronous_codec::Decoder as IoDecoder;
use bytes::BytesMut;
use forest_encoding::{de::DeserializeOwned, error::*};
use serde::Deserialize;
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

impl<Item: DeserializeOwned> IoDecoder for Decoder<Item> {
    type Item = Item;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Item>, Error> {
        // let mut pos = 0;
        let slice: &[u8] = src;
        let reader = BufReader::new(slice);

        match serde_ipld_dagcbor::de::from_reader(reader) {
            Ok(item) => Ok(Some(item)),
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
    fn test_data() -> TestData {
        let mut data = HashMap::new();
        data.insert("hello".to_owned(), 42usize);
        data.insert("world".to_owned(), 0usize);
        data
    }

    #[test]
    fn decode_success() {
        let mut decoder = Decoder::new();
        let data = test_data();
        let encoded = serde_ipld_dagcbor::to_vec(&data).unwrap();

        let mut all = BytesMut::with_capacity(encoded.len());
        all.extend(&encoded);
        let decoded = decoder.decode(&mut all).unwrap().unwrap();
        assert_eq!(data, decoded);
    }

    #[test]
    fn decode_failure() {
        let mut decoder = Decoder::<HashMap<String, usize>>::new();
        let data = test_data();
        let encoded = serde_ipld_dagcbor::to_vec(&data).unwrap();

        let mut all = BytesMut::with_capacity(encoded.len());
        // Put two copies into the buffer
        all.extend(&encoded);
        all.extend(&encoded);

        decoder.decode(&mut all).unwrap_err();
    }
}

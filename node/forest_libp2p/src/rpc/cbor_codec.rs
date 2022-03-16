// Code originally from futures_cbor_codec. License: Apache-2.0/MIT

use {
    asynchronous_codec::{Decoder as IoDecoder, Encoder as IoEncoder},
    bytes::{Buf, BytesMut},
    serde::{Deserialize, Serialize},
    serde_ipld_dagcbor::de::{Deserializer, IoRead},
    // serde_cbor::ser::{IoWrite, Serializer},
    // serde_cbor::{
    //     de::{Deserializer, IoRead},
    //     error::Error as CborError,
    // },
    serde_ipld_dagcbor::ser::{IoWrite, Serializer},
    serde_ipld_dagcbor::Error as CborError,
    std::{
        error::Error as ErrorTrait,
        fmt::{Display, Formatter, Result as FmtResult},
        io::{Error as IoError, Read, Result as IoResult, Write},
        marker::PhantomData,
    },
};

#[allow(variant_size_differences)]
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

/// Describes the behaviour of self-describe tags.
///
/// CBOR defines a tag which can be used to recognize a document as being CBOR (it's sometimes
/// called „magic“). This specifies if it should be present when encoding.
//
#[derive(Clone, Debug, Eq, PartialEq)]
//
pub enum SdMode {
    /// Places the tag in front of each encoded frame.
    _Always,
    /// Places the tag in front of the first encoded frame.
    Once,
    /// Doesn't place the tag at all.
    Never,
}

/// CBOR based encoder.
///
/// This encoder can be used with `future_codec`'s `FramedWrite` to encode CBOR frames. Anything
/// that is `serde`s `Serialize` can be encoded this way (at least in theory, some values return
/// errors when attempted to serialize).
//
#[derive(Clone, Debug)]
//
pub struct Encoder<Item> {
    _data: PhantomData<fn(Item)>,
    sd: SdMode,
    packed: bool,
}

impl<Item: Serialize> Encoder<Item> {
    /// Creates a new encoder.
    ///
    /// By default, it doesn't do packed encoding (it includes struct field names) and it doesn't
    /// prefix the frames with self-describe tag.
    //
    pub fn new() -> Self {
        Self {
            _data: PhantomData,
            sd: SdMode::Never,
            packed: false,
        }
    }

    /// Turns the encoder into one with configured self-describe behaviour.
    //
    pub fn _sd(self, sd: SdMode) -> Self {
        Self { sd, ..self }
    }

    /// Turns the encoder into one with configured packed encoding.
    ///
    /// If `packed` is true, it omits the field names from the encoded data. That makes it smaller,
    /// but it also means the decoding end must know the exact order of fields and it can't be
    /// something like python, which would want to get a dictionary out of it.
    //
    pub fn _packed(self, packed: bool) -> Self {
        Self { packed, ..self }
    }
}

impl<Item: Serialize> Default for Encoder<Item> {
    fn default() -> Self {
        Self::new()
    }
}

/// The Cbor serializer wants a writer, we provide one by wrapping `BytesMut`.
///
/// As of writing this code, `BytesMut` doesn't know how to be a writer itself. This may change,
/// there's an open issue for it: https://github.com/carllerche/bytes/issues/77.
//
struct BytesWriter<'a>(&'a mut BytesMut);

impl Write for BytesWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.0.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

impl<Item: Serialize> IoEncoder for Encoder<Item> {
    type Item = Item;
    type Error = Error;

    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<(), Error> {
        let writer = BytesWriter(dst);

        let mut serializer = if self.packed {
            Serializer::new(IoWrite::new(writer)).packed_format()
        } else {
            Serializer::new(IoWrite::new(writer))
        };

        if self.sd != SdMode::Never {
            serializer.self_describe()?;
        }

        if self.sd == SdMode::Once {
            self.sd = SdMode::Never;
        }

        item.serialize(&mut serializer).map_err(Into::into)
    }
}

/// Cbor serializer and deserializer.
///
/// This is just a combined [`Decoder`](struct.Decoder.html) and [`Encoder`](struct.Encoder.html).
//
#[derive(Clone, Debug)]
//
pub struct Codec<Dec, Enc> {
    dec: Decoder<Dec>,
    enc: Encoder<Enc>,
}

impl<'de, Dec: Deserialize<'de>, Enc: Serialize> Codec<Dec, Enc> {
    /// Creates a new codec
    //
    pub fn new() -> Self {
        Self {
            dec: Decoder::new(),
            enc: Encoder::new(),
        }
    }

    /// Turns the internal encoder into one with confifured self-describe behaviour.
    //
    pub fn _sd(self, sd: SdMode) -> Self {
        Self {
            dec: self.dec,
            enc: Encoder { sd, ..self.enc },
        }
    }

    /// Turns the internal encoder into one with configured packed encoding.
    ///
    /// If `packed` is true, it omits the field names from the encoded data. That makes it smaller,
    /// but it also means the decoding end must know the exact order of fields and it can't be
    /// something like python, which would want to get a dictionary out of it.
    //
    pub fn _packed(self, packed: bool) -> Self {
        Self {
            dec: self.dec,
            enc: Encoder { packed, ..self.enc },
        }
    }
}

impl<'de, Dec: Deserialize<'de>, Enc: Serialize> Default for Codec<Dec, Enc> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de, Dec: Deserialize<'de>, Enc: Serialize> IoDecoder for Codec<Dec, Enc> {
    type Item = Dec;
    type Error = Error;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Dec>, Error> {
        self.dec.decode(src)
    }
}

impl<'de, Dec: Deserialize<'de>, Enc: Serialize> IoEncoder for Codec<Dec, Enc> {
    type Item = Enc;
    type Error = Error;
    fn encode(&mut self, item: Enc, dst: &mut BytesMut) -> Result<(), Error> {
        self.enc.encode(item, dst)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

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

    /// Run the decoding tests on the combined codec.
    #[test]
    fn decode_codec() {
        let decoder: Codec<_, ()> = Codec::new();
        decode(decoder);
    }

    /// Test encoding.
    fn encode<Enc: IoEncoder<Item = TestData, Error = Error>>(enc: Enc) {
        let mut encoder = enc;
        let data = test_data();
        let mut buffer = BytesMut::with_capacity(0);
        encoder.encode(data.clone(), &mut buffer).unwrap();
        let pos1 = buffer.len();
        let decoded = serde_ipld_dagcbor::from_slice::<TestData>(&buffer).unwrap();
        assert_eq!(data, decoded);
        // Once more, this time without the self-describe (should be smaller)
        encoder.encode(data.clone(), &mut buffer).unwrap();
        let pos2 = buffer.len();
        // More data arrived
        assert!(pos2 > pos1);
        // But not as much as twice as many
        assert!(pos1 * 2 > pos2);
        // We can still decode it
        let decoded = serde_ipld_dagcbor::from_slice::<TestData>(&buffer[pos1..]).unwrap();
        assert_eq!(data, decoded);
        // Encoding once more the size stays the same
        encoder.encode(data, &mut buffer).unwrap();
        let pos3 = buffer.len();
        assert_eq!(pos2 - pos1, pos3 - pos2);
    }

    /// Test encoding by the lone encoder.
    #[test]
    fn encode_only() {
        let encoder = Encoder::new()._sd(SdMode::Once);
        encode(encoder);
    }

    /// The same as `encode_only`, but with packed encoding.
    #[test]
    fn encode_packed() {
        let encoder = Encoder::new()._packed(true)._sd(SdMode::Once);
        encode(encoder);
    }

    /// Encoding with the combined `Codec`
    #[test]
    fn encode_codec() {
        let encoder: Codec<(), _> = Codec::new()._sd(SdMode::Once);
        encode(encoder);
    }

    /// Checks that the codec can be send
    #[test]
    fn is_send() {
        let codec: Codec<(), ()> = Codec::new();
        std::thread::spawn(move || {
            let _c = codec;
        });
    }

    /// Checks that the codec can be send
    #[test]
    fn is_sync() {
        let codec: Arc<Codec<(), ()>> = Arc::new(Codec::new());
        std::thread::spawn(move || {
            let _c = codec;
        });
    }
}

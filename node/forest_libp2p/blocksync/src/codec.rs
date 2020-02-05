use futures_codec::{Decoder, Encoder};
use futures::io::Error;

pub struct DagCborCodec;

impl Encoder for DagCborCodec {
    type Item = ();
    type Error = ();

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {

    }
}

impl Decoder for DagCborCodec {
    type Item = ();
    type Error = ();

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        unimplemented!()
    }
}
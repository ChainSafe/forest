use futures_codec::{Decoder, Encoder};
use futures::io::Error;

use super::blocksync_message;

struct InboundCodec;
struct OutboundCodec;




//impl Encoder for InboundCodec {
//    type Item = ();
//    type Error = ();
//
//    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
//        unimplemented!()
//    }
//}

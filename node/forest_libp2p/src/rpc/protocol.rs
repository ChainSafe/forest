use libp2p::core::{UpgradeInfo};
use libp2p::InboundUpgrade;
use futures::{AsyncWrite, AsyncRead};
use futures_codec::Framed;
use super::codec;

const MAX_RPC_SIZE: u64 = 4_194_304;



#[derive(Debug, Clone)]
pub struct RPCProtocol;


impl UpgradeInfo for RPCProtocol {
    type Info = &'static [u8];
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        vec![
            b"/fil/sync/blk/0.0.1"
        ]
    }
}

//impl<TSocket> InboundUpgrade <TSocket> for RPCProtocol
//    where
//        TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
//{
//    type Output = Framed<TSocket, >;
//    type Error = ();
//    type Future = ();
//
//    fn upgrade_inbound(self, socket: TSocket, info: Self::Info) -> Self::Future {
//        unimplemented!()
//    }
//}
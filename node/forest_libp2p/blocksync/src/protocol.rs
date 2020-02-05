use libp2p::core::{
    upgrade::{self, Negotiated, ReadOneError},
    InboundUpgrade, OutboundUpgrade, UpgradeInfo,
};
use std::{io, iter};

use forest_encoding::{to_vec, from_slice};
use futures::future::BoxFuture;
use futures::io::AsyncReadExt;
use futures::prelude::*;

use super::message::{Message, Response, TipSetBundle};

#[derive(Clone, Debug, Default)]
pub struct BlockSyncConfig {}

impl UpgradeInfo for BlockSyncConfig {
    type Info = &'static [u8];
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(b"/fil/sync/blk/0.0.1")
    }
}

impl<C> InboundUpgrade<C> for BlockSyncConfig
where
    C: AsyncRead + AsyncWrite,
{
    type Output = Message;
    type Error = ReadOneError;
    type Future = BoxFuture<'static, Result<Self::Output, io::Error>>;


    fn upgrade_inbound(self, socket: Negotiated<C>, info: Self::Info) -> Self::Future {
        println!("upgrade_inbound: {}", std::str::from_utf8(&info).unwrap());
        upgrade::read_one_then(socket, 524288, (), |packet, ()| {
            let message: Message = Message {
                start: vec![],
                request_len: 0,
                options: 0,
            };
            println!("inbound message: {:?}", packet);
            Ok(message)
        })
    }
}

impl UpgradeInfo for Message {
    type Info = &'static [u8];
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(b"/fil/sync/blk/0.0.1")
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for Message
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = Response;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, io::Error>>;

    #[inline]
    fn upgrade_outbound(self, mut socket: TSocket, info: Self::Info) -> Self::Future {
        println!("upgrade_outbound: {}", std::str::from_utf8(info).unwrap());
        let payload = to_vec(&self).unwrap();
        async move {
            socket.write_all(&payload).await?;
            socket.close().await?;
            let mut buf: Vec<u8> = vec![];
            socket.read_to_end(&mut buf);
            let (chain, status, message) = from_slice(buf).unwrap();
            Ok(Response {
                chain,
                status,
                message,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::from_slice;
    use encoding::to_vec;
    use ferret_cid as cid;

    #[test]
    fn serialize() {
        let bytes: Vec<u8> = vec![
            0x83, 0x81, 0xd8, 0x2a, 0x58, 0x27, 0x0, 0x1, 0x71, 0xa0, 0xe4, 0x2, 0x20, 0xf4, 0x49,
            0x23, 0x61, 0xba, 0x18, 0xef, 0xa4, 0x99, 0x30, 0xd3, 0x75, 0x59, 0xe8, 0xa7, 0x2a,
            0xe6, 0x6e, 0x35, 0x37, 0x38, 0x24, 0x11, 0xa5, 0x90, 0xcc, 0xb5, 0x9c, 0x94, 0x43,
            0xce, 0x42, 0x1, 0x1,
        ];
        let cid = "BAFY2BZACED2ESI3BXIMO7JEZGDJXKWPIU4VOM3RVG44CIENFSDGLLHEUIPHEE".to_owned();
        let cid = cid.to_cid().unwrap();

        println!("cid: {:?}", cid.to_bytes().len());
        let req = Message {
            start: vec![cid],
            request_len: 1,
            options: 1,
        };
        let payload3 = to_vec(&req).unwrap();
        assert_eq!(payload3, bytes);
        let deserialized: Message = from_slice(&bytes).unwrap();
        assert_eq!(deserialized, req);
    }
    #[test]
    fn deser() {
        let payload: Vec<u8> = vec![
            131, 129, 133, 129, 141, 67, 0, 205, 2, 129, 88, 96, 185, 135, 167, 190, 128, 155, 31,
            220, 130, 243, 33, 113, 97, 248, 77, 226, 8, 217, 184, 244, 69, 207, 85, 129, 235, 161,
            64, 246, 9, 60, 178, 45, 175, 76, 108, 77, 219, 117, 53, 26, 195, 104, 231, 32, 151,
            241, 111, 134, 2, 176, 156, 198, 143, 123, 26, 18, 76, 250, 6, 117, 135, 211, 210, 0,
            75, 26, 93, 3, 57, 32, 120, 36, 146, 147, 2, 57, 135, 149, 227, 99, 40, 96, 138, 85,
            154, 102, 210, 16, 87, 238, 48, 134, 128, 113, 34, 155, 131, 88, 192, 179, 147, 71,
            121, 41, 230, 250, 95, 124, 128, 194, 208, 221, 39, 109, 255, 39, 59, 133, 239, 149,
            40, 61, 36, 49, 197, 121, 117, 78, 142, 172, 89, 75, 53, 108, 109, 6, 7, 61, 1, 218,
            162, 221, 230, 21, 41, 0, 237, 139, 236, 138, 192, 168, 178, 216, 49, 105, 133, 125,
            251, 209, 163, 15, 230, 127, 167, 81, 113, 241, 180, 129, 213, 74, 178, 152, 92, 210,
            83, 200, 90, 3, 225, 38, 201, 38, 75, 164, 119, 184, 134, 193, 55, 171, 164, 9, 151, 7,
            128, 186, 110, 6, 81, 130, 210, 38, 15, 99, 76, 41, 41, 213, 105, 111, 144, 166, 117,
            150, 35, 239, 0, 4, 59, 35, 114, 113, 6, 110, 222, 164, 88, 45, 238, 117, 183, 108,
            230, 61, 239, 250, 202, 249, 211, 12, 252, 184, 22, 90, 102, 132, 164, 156, 170, 246,
            170, 43, 99, 203, 98, 35, 237, 87, 243, 16, 142, 26, 206, 52, 103, 237, 145, 72, 219,
            125, 149, 142, 158, 23, 221, 190, 165, 121, 230, 221, 71, 176, 24, 59, 99, 118, 233,
            177, 230, 88, 96, 184, 232, 166, 35, 123, 113, 214, 75, 68, 54, 84, 199, 138, 203, 146,
            130, 56, 90, 157, 197, 45, 172, 215, 145, 45, 218, 124, 253, 26, 146, 78, 33, 130, 113,
            74, 123, 229, 119, 143, 60, 248, 18, 96, 221, 107, 203, 221, 190, 14, 149, 234, 88, 42,
            195, 124, 204, 70, 157, 251, 254, 166, 245, 129, 38, 196, 195, 80, 93, 64, 129, 202,
            233, 86, 179, 17, 201, 251, 251, 111, 27, 246, 92, 159, 228, 204, 70, 45, 42, 10, 191,
            255, 192, 0, 19, 115, 151, 129, 131, 88, 32, 169, 159, 229, 76, 60, 56, 177, 189, 69,
            46, 231, 23, 176, 53, 134, 20, 226, 135, 61, 88, 6, 146, 155, 222, 97, 121, 173, 92,
            172, 201, 124, 13, 25, 28, 1, 0, 131, 216, 42, 88, 39, 0, 1, 113, 160, 228, 2, 32, 88,
            62, 211, 126, 205, 245, 109, 107, 37, 183, 11, 52, 236, 209, 162, 158, 200, 223, 237,
            43, 243, 174, 137, 246, 247, 7, 1, 12, 29, 85, 250, 67, 216, 42, 88, 39, 0, 1, 113,
            160, 228, 2, 32, 16, 159, 196, 54, 13, 45, 59, 251, 119, 168, 252, 230, 110, 33, 161,
            145, 57, 31, 196, 2, 253, 195, 116, 146, 41, 80, 37, 155, 95, 135, 202, 124, 216, 42,
            88, 39, 0, 1, 113, 160, 228, 2, 32, 174, 133, 66, 93, 220, 135, 221, 50, 242, 139, 84,
            151, 230, 77, 216, 129, 60, 247, 170, 43, 238, 207, 73, 158, 58, 208, 189, 110, 72,
            196, 195, 171, 69, 0, 1, 8, 135, 110, 25, 5, 217, 216, 42, 88, 39, 0, 1, 113, 160, 228,
            2, 32, 71, 104, 246, 146, 241, 120, 45, 182, 208, 45, 45, 134, 238, 193, 191, 245, 109,
            205, 201, 18, 187, 87, 107, 116, 137, 57, 100, 47, 214, 64, 46, 143, 216, 42, 88, 39,
            0, 1, 113, 160, 228, 2, 32, 201, 98, 193, 85, 33, 199, 253, 158, 6, 190, 75, 79, 206,
            128, 166, 44, 246, 150, 213, 174, 167, 104, 120, 25, 91, 216, 26, 11, 100, 102, 204,
            100, 216, 42, 88, 39, 0, 1, 113, 160, 228, 2, 32, 181, 175, 212, 118, 108, 3, 119, 123,
            164, 217, 8, 235, 160, 45, 88, 201, 209, 152, 138, 245, 147, 21, 99, 72, 169, 82, 33,
            244, 140, 216, 4, 253, 88, 97, 2, 173, 92, 36, 245, 48, 181, 241, 48, 18, 3, 214, 65,
            41, 231, 100, 229, 223, 57, 254, 145, 129, 127, 222, 188, 8, 224, 35, 78, 9, 10, 189,
            232, 8, 159, 63, 231, 36, 118, 83, 10, 23, 169, 2, 187, 28, 175, 34, 148, 20, 121, 208,
            22, 158, 119, 90, 3, 66, 238, 182, 254, 91, 11, 176, 151, 72, 38, 47, 150, 172, 241,
            129, 146, 217, 74, 51, 219, 144, 213, 185, 163, 145, 144, 199, 249, 186, 52, 101, 24,
            182, 131, 94, 32, 128, 115, 198, 97, 26, 94, 30, 25, 181, 88, 97, 2, 181, 44, 23, 10,
            234, 52, 241, 153, 164, 14, 139, 218, 5, 151, 174, 169, 109, 230, 248, 246, 21, 103,
            91, 221, 140, 137, 118, 32, 89, 122, 231, 90, 6, 198, 131, 183, 150, 51, 202, 95, 238,
            103, 161, 172, 164, 200, 102, 7, 6, 184, 175, 122, 87, 41, 86, 201, 194, 254, 16, 9,
            179, 55, 168, 29, 65, 96, 89, 214, 101, 211, 5, 97, 66, 80, 227, 36, 56, 186, 146, 112,
            214, 99, 182, 117, 233, 64, 105, 35, 107, 189, 111, 227, 115, 197, 169, 154, 0, 128,
            128, 128, 128, 0, 96,
        ];
        let deserialized: Response = from_slice(&payload).unwrap();
    }
}

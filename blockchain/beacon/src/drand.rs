pub use crate::api::*;
pub use crate::api_grpc::PublicClient;
pub use crate::common::{GroupPacket as ProtoGroup, GroupRequest, Identity as ProtoIdentity};
use grpc::ClientStub;
use grpc::RequestOptions;

use crate::group::Group;
use httpbis::ClientTlsOption;
use std::convert::TryFrom;
use std::error;
use std::sync::Arc;
use tls_api::*;
use tls_api_openssl::*;
struct DrandPeer {
    addr: String,
}
pub struct DrandBeacon {
    client: PublicClient,
    peers: Vec<DrandPeer>,

    // pubkey
    interval: u64,

    drand_gen_time: u64,

    fil_gen_time: u64,
    fil_round_time: u64,
}

impl DrandBeacon {
    pub async fn new(
        genesis_ts: u64,
        interval: u64,
    ) -> std::result::Result<Self, Box<dyn error::Error>> {
        if genesis_ts == 0 {
            panic!("what are you doing this cant be zero")
        }

        // construct grpc client
        let client = grpc::ClientBuilder::new("drand-test1.nikkolasg.xyz", 5001)
            .tls::<tls_api_openssl::TlsConnector>()
            .build()
            .unwrap();
        let client = PublicClient::with_client(Arc::new(client));

        // peers append all peers

        // get nodes in group
        let req = GroupRequest::new();
        let group_resp = client
            .group(RequestOptions::new(), req)
            .drop_metadata()
            .await?;
        let group: Group = Group::try_from(group_resp)?;
        todo!();
    }
}

#[cfg(test)]
mod test {
    use tls_api::*;
    use tls_api_openssl::*;
    use httpbis::ClientTlsOption;
    use std::sync::Arc;
    pub use crate::api_grpc::PublicClient;
    use grpc::ClientStub;
    pub use crate::common::{GroupRequest};
    use async_std::prelude::*;
    use crate::group::Group;
    use std::convert::TryFrom;

    #[async_std::test]
    async fn t1() {
        let client = grpc::ClientBuilder::new("drand-test1.nikkolasg.xyz", 5001)
        .tls::<tls_api_openssl::TlsConnector>()
        .build()
        .unwrap();
        let client = PublicClient::with_client(Arc::new(client));

        let req = GroupRequest::new();
        let resp = client.group(grpc::RequestOptions::new(), req).drop_metadata().await.unwrap();

        let group: Group = Group::try_from(resp).unwrap();
        println!("{:?}", group);
    }
}
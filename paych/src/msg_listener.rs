// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Error;
use cid::Cid;
use flo_stream::{MessagePublisher, Publisher, Subscriber};

// convert this to a trait to allow for different implementations
pub struct MsgListeners {
    ps: Publisher<MsgCompleteEvt>,
    num_pubs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsgCompleteEvt {
    mcid: Cid,
    err: String,
}

impl Default for MsgListeners {
    fn default() -> Self {
        Self::new()
    }
}

impl MsgListeners {
    pub fn new() -> Self {
        MsgListeners {
            ps: Publisher::new(50),
            num_pubs: 0,
        }
    }

    pub async fn subscribe(&mut self) -> Subscriber<MsgCompleteEvt> {
        self.ps.subscribe()
    }

    pub async fn fire_msg_complete(&mut self, mcid: Cid, err: Error) {
        self.num_pubs += 1;
        self.ps
            .publish(MsgCompleteEvt {
                mcid,
                err: err.to_string(),
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::task;
    use futures::StreamExt;

    fn test_cids() -> Vec<Cid> {
        let cid1 = Cid::from_raw_cid("QmdmGQmRgRjazArukTbsXuuxmSHsMCcRYPAZoGhd6e3MuS").unwrap();
        let cid2 = Cid::from_raw_cid("QmdvGCmN6YehBxS6Pyd991AiQRJ1ioqcvDsKGP2siJCTDL").unwrap();
        vec![cid1, cid2]
    }

    #[test]
    fn test_msg_listener() {
        task::block_on(async {
            let mut ml = MsgListeners::new();

            let done = false;
            let cid = Cid::from_raw_cid("QmdmGQmRgRjazArukTbsXuuxmSHsMCcRYPAZoGhd6e3MuS").unwrap();
            let mut sub = ml.subscribe().await;
            ml.fire_msg_complete(
                cid.clone(),
                Error::Other("this should not be an error".to_string()),
            )
            .await;

            assert_eq!(sub.next().await.unwrap().mcid, cid)
        })
    }
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use flo_stream::{MessagePublisher, Publisher, Subscriber};

pub struct MsgListeners {
    ps: Publisher<MsgCompleteEvt>,
    num_pubs: u64,
}

pub type MsgCompleteEvt = Result<Cid, String>;

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

    /// called when a message completes
    pub async fn fire_msg_complete(&mut self, mcid: Cid) {
        self.num_pubs += 1;
        let msg_complete: MsgCompleteEvt = Ok(mcid);

        self.ps.publish(msg_complete).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::task;
    use futures::StreamExt;

    fn _test_cids() -> Vec<Cid> {
        let cid1 = Cid::from_raw_cid("QmdmGQmRgRjazArukTbsXuuxmSHsMCcRYPAZoGhd6e3MuS").unwrap();
        let cid2 = Cid::from_raw_cid("QmdvGCmN6YehBxS6Pyd991AiQRJ1ioqcvDsKGP2siJCTDL").unwrap();
        vec![cid1, cid2]
    }

    #[test]
    fn test_msg_listener() {
        task::block_on(async {
            let mut ml = MsgListeners::new();

            let cid = Cid::from_raw_cid("QmdmGQmRgRjazArukTbsXuuxmSHsMCcRYPAZoGhd6e3MuS").unwrap();
            let mut sub = ml.subscribe().await;

            ml.fire_msg_complete(cid.clone()).await;

            match sub.next().await.unwrap() {
                Ok(c) => assert_eq!(c, cid),
                _ => println!("error"),
            };
        })
    }
}

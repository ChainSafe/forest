// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{self, prelude::*};
use std::time::{Duration, Instant};

#[derive(Debug, clap::Args)]
pub struct WaitApiCommand {
    /// duration to wait till fail, e.g. `5s`, `5seconds`, `1m`, `1min`, etc.
    #[arg(long)]
    timeout: Option<humantime::Duration>,
}

impl WaitApiCommand {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        let request = Version::request(())?.with_timeout(Duration::from_secs(1));
        let timeout = self.timeout.map(Duration::from);
        let start = Instant::now();
        let mut success = false;
        loop {
            match timeout {
                Some(timeout) if start.elapsed() > timeout => break,
                _ => {}
            }
            if let Ok(_r) = client.call(request.clone()).await {
                success = true;
                break;
            }
            println!("Not online yet...");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        if success {
            println!("Forest API is online!");
        } else {
            println!("Timed out waiting for the API to come online");
        }

        Ok(())
    }
}

// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{self, prelude::*};
use std::time::Duration;

#[derive(Debug, clap::Args)]
pub struct WaitApiCommand {
    /// duration to wait till fail
    #[arg(long)]
    timeout: Option<humantime::Duration>,
}

impl WaitApiCommand {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        let request = Version::request(())?.with_timeout(Duration::from_secs(1));
        let deadline = self.timeout.map(|timeout| {
            chrono::Utc::now()
                .checked_add_signed(
                    chrono::Duration::from_std(timeout.into())
                        .expect("Failed to convert humantime::Duration to std::time::Duration"),
                )
                .expect("Failed to calculate deadline")
        });

        let mut success = false;
        loop {
            match deadline {
                Some(deadline) if chrono::Utc::now() > deadline => break,
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

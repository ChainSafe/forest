// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{self, prelude::*};
use std::time::Duration;

#[derive(Debug, clap::Args)]
pub struct WaitApiCommand {
    /// duration to wait till fail
    #[arg(long, default_value = "30s")]
    timeout: humantime::Duration,
}

impl WaitApiCommand {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        let request = Version::request(())?.with_timeout(Duration::from_secs(1));
        let deadline = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::from_std(self.timeout.into())?)
            .expect("Failed to calculate deadline");

        let mut success = false;
        while chrono::Utc::now() <= deadline {
            if let Ok(_r) = client.call(request.clone()).await {
                success = true;
                break;
            }
            println!("Not online yet...");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        if success {
            println!("Forest API has been online!");
        } else {
            println!("Timed out waiting for api to come online");
        }

        Ok(())
    }
}

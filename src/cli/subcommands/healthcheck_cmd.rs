// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    io::{stdout, Write},
    time::Duration,
};

use crate::health::DEFAULT_HEALTHCHECK_PORT;
use crate::rpc;
use clap::Subcommand;
use http::StatusCode;
use ticker::Ticker;

#[derive(Debug, Subcommand)]
pub enum HealthcheckCommand {
    /// Display ready status
    Ready {
        /// Don't exit until node is ready
        #[arg(long)]
        wait: bool,
        /// Healthcheck port
        #[arg(long, default_value_t=DEFAULT_HEALTHCHECK_PORT)]
        healthcheck_port: u16,
    },
}

impl HealthcheckCommand {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Ready {
                wait,
                healthcheck_port,
            } => {
                let ticker = Ticker::new(0.., Duration::from_secs(1));
                let mut stdout = stdout();

                let url = format!(
                    "http://{}:{}/readyz?verbose",
                    client.base_url().host_str().unwrap_or("localhost"),
                    healthcheck_port,
                );

                for _ in ticker {
                    let response = reqwest::get(&url).await?;
                    let status = response.status();
                    let text = response.text().await?;

                    println!("{}", text);

                    if !wait {
                        break;
                    }
                    if status == StatusCode::OK {
                        println!("Done!");
                        break;
                    }

                    for _ in 0..(text.matches('\n').count() + 1) {
                        write!(
                            stdout,
                            "\r{}{}",
                            anes::MoveCursorUp(1),
                            anes::ClearLine::All,
                        )?;
                    }
                }
                Ok(())
            }
        }
    }
}

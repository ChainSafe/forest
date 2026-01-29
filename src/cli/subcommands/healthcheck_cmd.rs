// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    io::{Write, stdout},
    time::Duration,
};

use crate::health::DEFAULT_HEALTHCHECK_PORT;
use crate::rpc;
use clap::Subcommand;
use http::StatusCode;

#[derive(Debug, Subcommand)]
pub enum HealthcheckCommand {
    /// Display readiness status
    Ready {
        /// Don't exit until node is ready
        #[arg(long)]
        wait: bool,
        /// Healthcheck port
        #[arg(long, default_value_t=DEFAULT_HEALTHCHECK_PORT)]
        healthcheck_port: u16,
    },
    /// Display liveness status
    Live {
        /// Don't exit until node is ready
        #[arg(long)]
        wait: bool,
        /// Healthcheck port
        #[arg(long, default_value_t=DEFAULT_HEALTHCHECK_PORT)]
        healthcheck_port: u16,
    },
    /// Display health status
    Healthy {
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
            } => Self::check(&client, "readyz", healthcheck_port, wait).await,
            Self::Live {
                wait,
                healthcheck_port,
            } => Self::check(&client, "livez", healthcheck_port, wait).await,
            Self::Healthy {
                wait,
                healthcheck_port,
            } => Self::check(&client, "healthz", healthcheck_port, wait).await,
        }
    }

    async fn check(
        client: &rpc::Client,
        endpoint: &str,
        healthcheck_port: u16,
        wait: bool,
    ) -> anyhow::Result<()> {
        let mut stdout = stdout();

        let url = format!(
            "http://{}:{healthcheck_port}/{endpoint}?verbose",
            client.base_url().host_str().unwrap_or("localhost"),
        );

        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let (status, text) = {
                match reqwest::get(&url).await {
                    Ok(response) => {
                        let status = response.status();
                        let text = match response.text().await {
                            Ok(t) => t,
                            Err(e) if wait => e.to_string(),
                            Err(e) => anyhow::bail!("{e}"),
                        };
                        (status, text)
                    }
                    Err(e) if wait => {
                        eprintln!("{e}");
                        (http::StatusCode::INTERNAL_SERVER_ERROR, "".into())
                    }
                    Err(e) => anyhow::bail!("{e}"),
                }
            };

            println!("{text}");

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

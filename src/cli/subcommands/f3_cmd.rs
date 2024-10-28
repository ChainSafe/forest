// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{self, f3::F3Manifest, prelude::*};
use cid::Cid;
use clap::{Subcommand, ValueEnum};
use sailfish::TemplateSimple;
use serde::{Deserialize, Serialize};

/// Output format
#[derive(ValueEnum, Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum F3OutputFormat {
    /// Text
    #[default]
    Text,
    /// JSON
    Json,
}

/// Manages Filecoin Fast Finality (F3) interactions
#[derive(Debug, Subcommand)]
pub enum F3Commands {
    /// Gets the current manifest used by F3
    Manifest {
        /// The output format.
        #[arg(long, value_enum, default_value_t = F3OutputFormat::Text)]
        output: F3OutputFormat,
    },
}

impl F3Commands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Manifest { output } => {
                let manifest = client.call(F3GetManifest::request(())?).await?;
                match output {
                    F3OutputFormat::Text => {
                        let template = ManifestTemplate::new(manifest);
                        println!("{}", template.render_once()?);
                    }
                    F3OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&manifest)?);
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(TemplateSimple, Debug, Clone, Serialize, Deserialize)]
#[template(path = "cli/f3/manifest.stpl")]
struct ManifestTemplate {
    manifest: F3Manifest,
    is_initial_power_table_defined: bool,
}

impl ManifestTemplate {
    fn new(manifest: F3Manifest) -> Self {
        let is_initial_power_table_defined = manifest.initial_power_table != Cid::default();
        Self {
            manifest,
            is_initial_power_table_defined,
        }
    }
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_types::genesis;
use std::fs::File;
use structopt::StructOpt;
use uuid::Uuid;

#[derive(Debug, StructOpt)]
pub enum GenesisCommands {
    /// Creates new genesis template
    #[structopt(about = "Create new Genesis template")]
    NewTemplate {
        #[structopt(short, help = "Input a network name")]
        network_name: Option<String>,
        #[structopt(
            short,
            default_value = "genesis.json",
            help = "File path, i.e, './genesis.json'. This command WILL NOT create a directory if it does not exist."
        )]
        file_path: String,
    },
}

impl GenesisCommands {
    pub async fn run(&self) {
        match self {
            Self::NewTemplate {
                network_name,
                file_path,
            } => {
                let template = genesis::Template::new(
                    network_name
                        .as_ref()
                        .unwrap_or(&format!("localnet-{}", Uuid::new_v4().to_string()))
                        .to_string(),
                );

                match &File::create(file_path) {
                    Ok(file) => {
                        serde_json::to_writer_pretty(file, &template).unwrap();
                    }
                    Err(err) => {
                        println!("Can not write to a file, error: {}", err);
                    }
                }
            }
        }
    }
}

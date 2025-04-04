// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{
    self, RpcMethodExt,
    eth::types::EthAddress,
    f3::{F3Manifest, GetManifestFromContract},
    state::StateCall,
};
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum F3Commands {
    CheckActivation {
        /// Contract eth address
        #[arg(long, required = true)]
        contract: EthAddress,
    },
    /// Queries F3 parameters contract using raw logic
    CheckActivationRaw {
        /// Contract eth address
        #[arg(long, required = true)]
        contract: EthAddress,
    },
}

impl F3Commands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::CheckActivation { contract: _ } => {
                unimplemented!("Will be done in a subsequent PR");
            }
            Self::CheckActivationRaw { contract } => {
                let eth_call_message = GetManifestFromContract::create_eth_call_message(contract);
                let api_invoc_result =
                    StateCall::call(&client, (eth_call_message.try_into()?, None.into())).await?;
                let Some(message_receipt) = api_invoc_result.msg_rct else {
                    anyhow::bail!("No message receipt");
                };
                let eth_return = F3Manifest::get_eth_return_from_message_receipt(&message_receipt)?;
                println!("Raw data: {}", hex::encode(eth_return.as_slice()));
                let manifest = F3Manifest::parse_contract_return(&eth_return)?;
                println!("{}", serde_json::to_string_pretty(&manifest)?);
            }
        }
        Ok(())
    }
}

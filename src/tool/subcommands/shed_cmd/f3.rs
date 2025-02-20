// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    rpc::{
        self,
        eth::types::{EthAddress, EthBytes, EthCallMessage},
        f3::F3Manifest,
        state::StateCall,
        RpcMethodExt,
    },
    shim::message::Message,
};
use clap::Subcommand;
use flate2::read::DeflateDecoder;
use std::{io::Read, str::FromStr as _};

const MAX_PAYLOAD_LEN: usize = 4 << 10;

#[derive(Debug, Subcommand)]
pub enum F3Commands {
    CheckActivation {
        /// Contract eth address
        #[arg(long, required = true)]
        contract: EthAddress,
    },
    CheckActivationRaw {
        /// Contract eth address
        #[arg(long, required = true)]
        contract: EthAddress,
    },
}

impl F3Commands {
    #[allow(clippy::indexing_slicing)]
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::CheckActivation { contract: _ } => {
                unimplemented!("Will be done in a subsequent PR");
            }
            Self::CheckActivationRaw { contract } => {
                let eth_call_message = EthCallMessage {
                    to: Some(contract),
                    data: EthBytes::from_str("0x2587660d")?, // method ID of activationInformation()
                    ..Default::default()
                };
                let filecoin_message = Message::try_from(eth_call_message)?;
                let api_invoc_result =
                    StateCall::call(&client, (filecoin_message, None.into())).await?;
                let Some(message_receipt) = api_invoc_result.msg_rct else {
                    anyhow::bail!("No message receipt");
                };
                anyhow::ensure!(
                    message_receipt.exit_code().is_success(),
                    "unsuccessful exit code {}",
                    message_receipt.exit_code()
                );
                let return_data = message_receipt.return_data();
                let eth_return =
                    fvm_ipld_encoding::from_slice::<fvm_ipld_encoding::BytesDe>(&return_data)?.0;
                println!("Raw data: {}", hex::encode(eth_return.as_slice()));
                // 3*32 because there should be 3 slots minimum
                anyhow::ensure!(eth_return.len() >= 3 * 32, "no activation information");
                let mut activation_epoch_bytes: [u8; 8] = [0; 8];
                activation_epoch_bytes.copy_from_slice(&eth_return[24..32]);
                let activation_epoch = u64::from_be_bytes(activation_epoch_bytes);
                for (i, &v) in eth_return[32..63].iter().enumerate() {
                    anyhow::ensure!(
                        v == 0,
                        "wrong value for offset (padding): slot[{i}] = 0x{v:x} != 0x00"
                    );
                }
                anyhow::ensure!(
                    eth_return[63] == 0x40,
                    "wrong value for offest : slot[31] = 0x{:x} != 0x40",
                    eth_return[63]
                );
                let mut payload_len_bytes: [u8; 8] = [0; 8];
                payload_len_bytes.copy_from_slice(&eth_return[88..96]);
                let payload_len = u64::from_be_bytes(payload_len_bytes) as usize;
                anyhow::ensure!(
                    payload_len <= MAX_PAYLOAD_LEN,
                    "too long declared payload: {payload_len} > {MAX_PAYLOAD_LEN}"
                );
                let payload_bytes = &eth_return[96..];
                anyhow::ensure!(
                    payload_len <= payload_bytes.len(),
                    "not enough remaining bytes: {payload_len} > {}",
                    payload_bytes.len()
                );
                anyhow::ensure!(
                    activation_epoch < u64::MAX && payload_len > 0,
                    "no active activation"
                );
                let compressed_manifest_bytes = &payload_bytes[..payload_len];
                let mut deflater = DeflateDecoder::new(compressed_manifest_bytes);
                let mut manifest_bytes = vec![];
                deflater.read_to_end(&mut manifest_bytes)?;
                let manifest: F3Manifest = serde_json::from_slice(&manifest_bytes)?;
                anyhow::ensure!(
                    manifest.bootstrap_epoch >= 0
                        && manifest.bootstrap_epoch as u64 == activation_epoch,
                    "bootstrap epoch does not match: {} != {activation_epoch}",
                    manifest.bootstrap_epoch
                );
                println!("{}", serde_json::to_string_pretty(&manifest)?);
            }
        }
        Ok(())
    }
}

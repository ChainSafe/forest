// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::types::{EthAddress, EthBytes};
use crate::rpc::state::{MessageTrace, ReturnTrace};
use crate::shim::address::Address as FilecoinAddress;
use crate::shim::state_tree::StateTree;

use anyhow::{bail, Result};
use cbor4ii::core::dec::Decode as _;
use cbor4ii::core::Value;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{RawBytes, CBOR, DAG_CBOR, IPLD_RAW};
use serde::de;

/// The Identity multicodec code.
pub const IDENTITY: u64 = 0x00;

pub fn lookup_eth_address<DB: Blockstore>(
    addr: &FilecoinAddress,
    state: &StateTree<DB>,
) -> Result<Option<EthAddress>> {
    // Attempt to convert directly, if it's an f4 address.
    if let Ok(eth_addr) = EthAddress::from_filecoin_address(addr) {
        if !eth_addr.is_masked_id() {
            return Ok(Some(eth_addr));
        }
    }

    // Otherwise, resolve the ID addr.
    let id_addr = match state.lookup_id(addr)? {
        Some(id) => id,
        _ => return Ok(None),
    };

    // Lookup on the target actor and try to get an f410 address.
    let result = state.get_actor(addr);
    if let Ok(Some(actor_state)) = result {
        if let Some(addr) = actor_state.delegated_address {
            if let Ok(eth_addr) = EthAddress::from_filecoin_address(&addr.into()) {
                if !eth_addr.is_masked_id() {
                    // Conversable into an eth address, use it.
                    return Ok(Some(eth_addr));
                }
            }
        } else {
            // No delegated address -> use a masked ID address
        }
    } else if let Ok(None) = result {
        // Not found -> use a masked ID address
    } else {
        // Any other error -> fail.
        result?;
    }

    // Otherwise, use the masked address.
    Ok(Some(EthAddress::from_actor_id(id_addr)))
}

/// Decodes the payload using the given codec.
pub fn decode_payload(payload: &RawBytes, codec: u64) -> Result<EthBytes> {
    match codec {
        IDENTITY => Ok(EthBytes::default()),
        DAG_CBOR | CBOR => {
            let mut reader = cbor4ii::core::utils::SliceReader::new(payload.bytes());
            match Value::decode(&mut reader) {
                Ok(Value::Bytes(bytes)) => Ok(EthBytes(bytes)),
                _ => bail!("failed to read params byte array"),
            }
        }
        IPLD_RAW => Ok(EthBytes(payload.to_vec())),
        _ => bail!("decode_payload: unsupported codec {codec}"),
    }
}

/// Decodes the message trace params using the message trace codec.
pub fn decode_params<'a, T>(trace: &'a MessageTrace) -> anyhow::Result<T>
where
    T: de::Deserialize<'a>,
{
    let codec = trace.params_codec;
    match codec {
        DAG_CBOR | CBOR => fvm_ipld_encoding::from_slice(&trace.params)
            .map_err(|e| anyhow::anyhow!("failed to decode params: {}", e)),
        _ => bail!("Method called an unexpected codec {codec}"),
    }
}

/// Decodes the return bytes using the return trace codec.
pub fn decode_return<'a, T>(trace: &'a ReturnTrace) -> anyhow::Result<T>
where
    T: de::Deserialize<'a>,
{
    let codec = trace.return_codec;
    match codec {
        DAG_CBOR | CBOR => fvm_ipld_encoding::from_slice(trace.r#return.bytes())
            .map_err(|e| anyhow::anyhow!("failed to decode return value: {}", e)),
        _ => bail!("Method returned an unexpected codec {codec}"),
    }
}

// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::types::{EthAddress, EthBytes};
use crate::rpc::state::{MessageTrace, ReturnTrace};
use crate::shim::address::Address as FilecoinAddress;
use crate::shim::fvm_shared_latest::IDENTITY_HASH;
use crate::shim::state_tree::StateTree;
use ahash::{HashMap, HashMapExt};

use anyhow::{bail, Result};
use cbor4ii::core::dec::Decode as _;
use cbor4ii::core::Value;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{RawBytes, CBOR, DAG_CBOR, IPLD_RAW};
use once_cell::sync::Lazy;
use serde::de;
use tracing::log;

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
        IDENTITY_HASH => Ok(EthBytes::default()),
        DAG_CBOR | CBOR => {
            let mut reader = cbor4ii::core::utils::SliceReader::new(payload.bytes());
            match Value::decode(&mut reader) {
                Ok(Value::Bytes(bytes)) => Ok(EthBytes(bytes)),
                _ => bail!("failed to decode params byte array"),
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

/// Extract and decode Ethereum revert reason from receipt return data
pub fn decode_revert_reason(return_data: RawBytes) -> (Vec<u8>, String) {
    let (data, reason) = match decode_payload(&return_data, CBOR) {
        Err(e) => {
            log::warn!("failed to unmarshal cbor bytes from message receipt return error: {e}");
            (
                EthBytes::default(),
                String::from("ERROR: revert reason is not cbor encoded bytes"),
            )
        }
        Ok(data) => (data.clone(), parse_eth_revert(data.0.as_slice())),
    };

    (data.0, reason)
}

const ERROR_FUNCTION_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0]; // Error(string)
const PANIC_FUNCTION_SELECTOR: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71]; // Panic(uint256)

// Lazily initialized HashMap for panic codes
static PANIC_ERROR_CODES: Lazy<HashMap<u64, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(0x00, "Panic()");
    m.insert(0x01, "Assert()");
    m.insert(0x11, "ArithmeticOverflow()");
    m.insert(0x12, "DivideByZero()");
    m.insert(0x21, "InvalidEnumVariant()");
    m.insert(0x22, "InvalidStorageArray()");
    m.insert(0x31, "PopEmptyArray()");
    m.insert(0x32, "ArrayIndexOutOfBounds()");
    m.insert(0x41, "OutOfMemory()");
    m.insert(0x51, "CalledUninitializedFunction()");
    m
});

/// Parse an ABI encoded revert reason from a raw return value.
///
/// Handles both `Error(string)` and `Panic(uint256)` formats according to
/// Solidity's revert conventions.
///
/// See https://docs.soliditylang.org/en/latest/control-structures.html#panic-via-assert-and-error-via-require
fn parse_eth_revert(data: &[u8]) -> String {
    // If it's not long enough to contain an ABI encoded response, return immediately.
    if data.len() < 4 + 32 {
        return format!("0x{}", hex::encode(data));
    }

    // Extract function selector (first 4 bytes)
    let selector = data.get(..4).expect("checked data length >= 4");

    match selector {
        selector if selector == PANIC_FUNCTION_SELECTOR.as_slice() => parse_panic_revert(data),
        selector if selector == ERROR_FUNCTION_SELECTOR.as_slice() => parse_error_revert(data),
        _ => format!("0x{}", hex::encode(data)),
    }
}

fn parse_error_revert(data: &[u8]) -> String {
    let data = match data.get(4..) {
        // Skip selector
        Some(d) if d.len() >= 32 => d,
        _ => return format!("0x{}", hex::encode(data)),
    };

    // Get offset safely
    let offset = data
        .get(28..32)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_be_bytes)
        .unwrap_or(0) as usize;

    // Validate offset range
    if offset >= data.len() || data.len().saturating_sub(offset) < 32 {
        return format!("0x{}", hex::encode(data));
    }

    // Get string length safely
    let len = data
        .get(offset + 28..offset + 32)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_be_bytes)
        .unwrap_or(0) as usize;

    let string_start = offset + 32;
    if string_start > data.len() || len > data.len() - string_start {
        return format!("0x{}", hex::encode(data));
    }

    // Attempt to decode valid UTF-8
    data.get(string_start..string_start + len)
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(|s| format!("Error({})", s))
        .unwrap_or_else(|| format!("0x{}", hex::encode(data)))
}

fn parse_panic_revert(data: &[u8]) -> String {
    let code_bytes = match data.get(4..36) {
        // Skip selector (4 bytes) + 32 bytes for panic code
        Some(b) => b,
        None => return format!("0x{}", hex::encode(data)),
    };

    // Check first 24 bytes are zero
    if code_bytes
        .get(..24)
        .map(|b| b.iter().all(|&v| v == 0))
        .unwrap_or(false)
    {
        code_bytes
            .get(24..32)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_be_bytes)
            .map(|code| {
                PANIC_ERROR_CODES
                    .get(&code)
                    .map(|&s| s.to_string())
                    .unwrap_or_else(|| format!("Panic(0x{:x})", code))
            })
            .unwrap_or_else(|| format!("Panic(0x{})", hex::encode(code_bytes)))
    } else {
        format!("Panic(0x{})", hex::encode(code_bytes))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cbor4ii::core::{enc::Encode, utils::BufWriter};
    use cbor4ii::serde::Serializer;

    #[test]
    fn test_decode_payload() {
        // empty
        let result = decode_payload(&RawBytes::default(), 0);
        assert!(result.unwrap().0.is_empty());

        // raw empty
        let result = decode_payload(&RawBytes::default(), IPLD_RAW);
        assert!(result.unwrap().0.is_empty());

        // raw non-empty
        let result = decode_payload(&RawBytes::new(vec![1]), IPLD_RAW);
        assert_eq!(result.unwrap(), EthBytes(vec![1]));

        // invalid cbor bytes
        let result = decode_payload(&RawBytes::default(), DAG_CBOR);
        assert!(result.is_err());

        // valid cbor bytes
        let mut writer = BufWriter::new(Vec::new());
        Value::Bytes(vec![1]).encode(&mut writer).unwrap();
        let serializer = Serializer::new(writer);
        let encoded = serializer.into_inner().into_inner();

        let result = decode_payload(&RawBytes::new(encoded.clone()), DAG_CBOR);
        assert_eq!(result.unwrap(), EthBytes(vec![1]));

        // regular cbor also works
        let result = decode_payload(&RawBytes::new(encoded), CBOR);
        assert_eq!(result.unwrap(), EthBytes(vec![1]));

        // random codec should fail
        let result = decode_payload(&RawBytes::default(), 42);
        assert!(result.is_err());

        // some payload taken from calibnet
        assert_eq!(
            decode_payload(
                &RawBytes::new(
                    hex::decode(
                        "58200000000000000000000000000000000000000000000000000000000000002710"
                    )
                    .unwrap(),
                ),
                CBOR
            )
            .unwrap(),
            EthBytes(vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 39, 16,
            ])
        );

        // identity
        let result = decode_payload(&RawBytes::new(vec![1]), IDENTITY_HASH);
        assert!(result.unwrap().0.is_empty());
    }
}

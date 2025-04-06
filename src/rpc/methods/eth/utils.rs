// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::types::{EthAddress, EthBytes};
use crate::rpc::state::{MessageTrace, ReturnTrace};
use crate::shim::address::Address as FilecoinAddress;
use crate::shim::fvm_shared_latest::IDENTITY_HASH;
use crate::shim::state_tree::StateTree;
use ahash::{HashMap, HashMapExt};

use crate::rpc::eth::EthUint64;
use anyhow::{Result, bail};
use cbor4ii::core::Value;
use cbor4ii::core::dec::Decode as _;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CBOR, DAG_CBOR, IPLD_RAW, RawBytes};
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

const ERROR_FUNCTION_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0]; // keccak256("Error(string)") [first 4 bytes]
const PANIC_FUNCTION_SELECTOR: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71]; // keccak256("Panic(uint256)") [first 4 bytes]

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

/// EVM error and panic related constants
const EVM_FUNC_SELECTOR_LENGTH: usize = 4;
const EVM_WORD_LENGTH: usize = 32;
const EVM_PANIC_CODE_LENGTH: usize = 32;
const EVM_UINT_PADDING_LENGTH: usize = 24;

/// Parse an ABI encoded revert reason from a raw return value.
///
/// Handles both `Error(string)` and `Panic(uint256)` formats according to
/// Solidity's revert conventions.
///
/// See https://docs.soliditylang.org/en/latest/control-structures.html#panic-via-assert-and-error-via-require
fn parse_eth_revert(data: &[u8]) -> String {
    // If it's not long enough to contain an ABI encoded response, return immediately.
    if data.len() < EVM_FUNC_SELECTOR_LENGTH + EVM_WORD_LENGTH {
        return format!("0x{}", hex::encode(data));
    }

    // Extract function selector (first 4 bytes)
    let selector = data
        .get(..EVM_FUNC_SELECTOR_LENGTH)
        .expect("checked data length >= 4");

    match selector {
        selector if selector == PANIC_FUNCTION_SELECTOR.as_slice() => parse_panic_revert(data),
        selector if selector == ERROR_FUNCTION_SELECTOR.as_slice() => parse_error_revert(data),
        _ => format!("0x{}", hex::encode(data)),
    }
}

fn parse_error_revert(data: &[u8]) -> String {
    let fallback = || format!("0x{}", hex::encode(data));

    let parse_result: Result<String, ()> = (|| {
        let data = data
            .get(EVM_FUNC_SELECTOR_LENGTH..)
            .filter(|d| d.len() >= EVM_WORD_LENGTH)
            .ok_or(())?;

        // Get offset, from the first 32 bytes of the data
        let offset_bytes = data.get(..EVM_WORD_LENGTH).ok_or(())?;
        let offset = EthUint64::from_bytes(offset_bytes).map_err(|_| ())?.0 as usize;

        // Validate offset range
        if offset >= data.len() || data.len().saturating_sub(offset) < EVM_WORD_LENGTH {
            return Err(());
        }

        // Get string length, from the offset + 32 bytes of the data
        let length_bytes = data.get(offset..offset + EVM_WORD_LENGTH).ok_or(())?;
        let len = EthUint64::from_bytes(length_bytes).map_err(|_| ())?.0 as usize;

        // Validate string length
        let string_start = offset + EVM_WORD_LENGTH;
        if string_start > data.len() || len > data.len() - string_start {
            return Err(());
        }

        // Attempt to decode valid UTF-8
        let string = data.get(string_start..string_start + len).ok_or(())?;
        Ok(format!(
            "Error({})",
            std::str::from_utf8(string).map_err(|_| ())?
        ))
    })();

    parse_result.unwrap_or_else(|_| fallback())
}

fn parse_panic_revert(data: &[u8]) -> String {
    let fallback = || format!("0x{}", hex::encode(data));

    let parse_result: Result<String, ()> = (|| {
        let code_bytes = data
            .get(EVM_FUNC_SELECTOR_LENGTH..EVM_FUNC_SELECTOR_LENGTH + EVM_PANIC_CODE_LENGTH)
            .ok_or(())?;

        // Check if first 24 bytes are all zeros
        if !code_bytes
            .get(..EVM_UINT_PADDING_LENGTH)
            .ok_or(())?
            .iter()
            .all(|&v| v == 0)
        {
            return Ok(format!("Panic(0x{})", hex::encode(code_bytes)));
        }

        let code_data = code_bytes.get(..EVM_WORD_LENGTH).ok_or(())?;
        let code = EthUint64::from_bytes(code_data).map_err(|_| ())?.0;
        Ok(PANIC_ERROR_CODES
            .get(&code)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Panic(0x{:x})", code)))
    })();

    parse_result.unwrap_or_else(|_| fallback())
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

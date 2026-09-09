// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::types::{EthAddress, EthBytes};
use crate::prelude::*;
use crate::rpc::state::{MessageTrace, ReturnTrace};
use crate::shim::actors::{EVMActorStateLoad as _, evm, is_evm_actor};
use crate::shim::address::Address as FilecoinAddress;
use crate::shim::fvm_shared_latest::IDENTITY_HASH;
use crate::shim::state_tree::{ActorState, StateTree};
use crate::utils::encoding::hex;
use ahash::HashMap;

use crate::rpc::eth::{EVM_WORD_LENGTH, EthUint64};
use crate::shim::actors::evm::U256;
use anyhow::{Result, bail};
use cbor4ii::core::Value;
use cbor4ii::core::dec::Decode as _;
use fvm_ipld_encoding::{CBOR, DAG_CBOR, IPLD_RAW, RawBytes};
use fvm_ipld_kamt::{AsHashedKey, Config as KamtConfig, HashedKey, Kamt};
use serde::de;
use std::borrow::Cow;
use std::sync::LazyLock;

/// KAMT configuration matching the EVM actor in builtin-actors.
// <https://github.com/filecoin-project/builtin-actors/blob/v18.0.0/actors/evm/src/interpreter/system.rs#L47>
pub(crate) fn evm_kamt_config() -> KamtConfig {
    KamtConfig {
        bit_width: 5,
        min_data_depth: 0,
        max_array_width: 1,
    }
}

/// Hash algorithm for the EVM storage KAMT: the key's big-endian bytes are the hash.
// <https://github.com/filecoin-project/builtin-actors/blob/v18.0.0/actors/evm/src/interpreter/system.rs#L49>
pub(crate) struct EvmStateHashAlgorithm;

impl AsHashedKey<U256, 32> for EvmStateHashAlgorithm {
    fn as_hashed_key(key: &U256) -> Cow<'_, HashedKey<32>> {
        Cow::Owned(key.to_big_endian())
    }
}

pub(crate) type EvmStorageKamt<BS> = Kamt<BS, U256, U256, EvmStateHashAlgorithm>;

pub fn lookup_eth_address<DB: Blockstore>(
    addr: &FilecoinAddress,
    state: &StateTree<DB>,
) -> Result<Option<EthAddress>> {
    // Attempt to convert directly, if it's an f4 address.
    if let Ok(eth_addr) = EthAddress::from_filecoin_address(addr)
        && !eth_addr.is_masked_id()
    {
        return Ok(Some(eth_addr));
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
            if let Ok(eth_addr) = EthAddress::from_filecoin_address(&addr.into())
                && !eth_addr.is_masked_id()
            {
                // Conversable into an eth address, use it.
                return Ok(Some(eth_addr));
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

/// The actor's EVM state, or `None` when it is not an EVM actor.
fn evm_state<DB: Blockstore>(store: &DB, actor: &ActorState) -> anyhow::Result<Option<evm::State>> {
    if !is_evm_actor(&actor.code) {
        return Ok(None);
    }
    Ok(Some(
        evm::State::load(store, actor.code, actor.state).context("failed to load EVM state")?,
    ))
}

/// As [`evm_state`], but also `None` for a dead (self-destructed) contract, which reads as empty.
// <https://github.com/filecoin-project/builtin-actors/blob/v18.0.0/actors/evm/src/interpreter/system.rs#L181>
pub(crate) fn live_evm_state<DB: Blockstore>(
    store: &DB,
    actor: &ActorState,
) -> anyhow::Result<Option<evm::State>> {
    Ok(evm_state(store, actor)?.filter(|state| state.is_alive()))
}

/// Extension trait for querying Ethereum-relevant state from a Filecoin actor.
pub(crate) trait ActorStateEthExt {
    /// Returns the effective nonce: EVM nonce for EVM actors (zero once self-destructed),
    /// sequence otherwise.
    fn eth_nonce<DB: Blockstore>(&self, store: &DB) -> anyhow::Result<EthUint64>;
    /// Returns the deployed bytecode of an EVM actor, or `None` for non-EVM or self-destructed actors.
    fn eth_bytecode<DB: Blockstore>(&self, store: &DB) -> anyhow::Result<Option<EthBytes>>;
    /// Returns the 32-byte storage value at `position`, matching the EVM actor's `GetStorageAt`: a
    /// zeroed word for a non-EVM actor, a dead (tombstoned) contract, or an unset slot.
    fn eth_storage_at<DB: Blockstore>(
        &self,
        store: &DB,
        position: &[u8; EVM_WORD_LENGTH],
    ) -> anyhow::Result<[u8; EVM_WORD_LENGTH]>;
}

impl ActorStateEthExt for ActorState {
    fn eth_nonce<DB: Blockstore>(&self, store: &DB) -> anyhow::Result<EthUint64> {
        Ok(EthUint64(match evm_state(store, self)? {
            Some(state) if state.is_alive() => state.nonce(),
            // A dead contract's state still carries a nonce, but it reports zero.
            Some(_) => 0,
            None => self.sequence,
        }))
    }

    fn eth_bytecode<DB: Blockstore>(&self, store: &DB) -> anyhow::Result<Option<EthBytes>> {
        let Some(evm_state) = live_evm_state(store, self)? else {
            return Ok(None);
        };
        let bytecode = store
            .get(&evm_state.bytecode())
            .context("failed to read EVM bytecode")?;
        Ok(bytecode.map(EthBytes))
    }

    fn eth_storage_at<DB: Blockstore>(
        &self,
        store: &DB,
        position: &[u8; EVM_WORD_LENGTH],
    ) -> anyhow::Result<[u8; EVM_WORD_LENGTH]> {
        // Mirrors the EVM actor's `GetStorageAt`.
        // <https://github.com/filecoin-project/builtin-actors/blob/v18.0.0/actors/evm/src/lib.rs#L309>
        let Some(evm_state) = live_evm_state(store, self)? else {
            return Ok([0; EVM_WORD_LENGTH]);
        };
        let kamt =
            EvmStorageKamt::load_with_config(&evm_state.contract_state(), store, evm_kamt_config())
                .context("failed to load EVM storage KAMT")?;
        let value = kamt
            .get(&U256::from_big_endian(position))
            .context("failed to read EVM storage slot")?
            .copied()
            .unwrap_or_default();
        Ok(value.to_big_endian())
    }
}

/// Decodes the payload using the given codec.
pub fn decode_payload(payload: &RawBytes, codec: u64) -> Result<EthBytes> {
    match codec {
        IDENTITY_HASH => Ok(EthBytes::default()),
        DAG_CBOR | CBOR => {
            let mut reader = cbor4ii::core::utils::SliceReader::new(payload.bytes());
            match Value::decode(&mut reader) {
                Ok(Value::Bytes(bytes)) => Ok(EthBytes(bytes)),
                other => {
                    tracing::debug!(
                        "failed to decode params byte array: {other:?}, codec: {codec}, payload: {}",
                        hex::encode(payload.bytes())
                    );
                    bail!("failed to decode params byte array");
                }
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
            .map_err(|e| anyhow::anyhow!("failed to decode params: {e}")),
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
            .map_err(|e| anyhow::anyhow!("failed to decode return value: {e}")),
        _ => bail!("Method returned an unexpected codec {codec}"),
    }
}

/// Extract and decode Ethereum revert reason from receipt return data
pub fn decode_revert_reason(return_data: RawBytes) -> (Vec<u8>, String) {
    match decode_payload(&return_data, CBOR) {
        Err(_) => (Vec::new(), String::new()),
        Ok(data) if !data.is_empty() => {
            let reason = parse_eth_revert(data.as_slice());
            (data.0, reason)
        }
        Ok(data) => (data.0, "none".to_string()),
    }
}

const ERROR_FUNCTION_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0]; // keccak256("Error(string)") [first 4 bytes]
const PANIC_FUNCTION_SELECTOR: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71]; // keccak256("Panic(uint256)") [first 4 bytes]

// Lazily initialized HashMap for panic codes
static PANIC_ERROR_CODES: LazyLock<HashMap<u64, &'static str>> = LazyLock::new(|| {
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
const EVM_PANIC_CODE_LENGTH: usize = 32;
const EVM_UINT_PADDING_LENGTH: usize = 24;

/// Parse an ABI encoded revert reason from a raw return value.
///
/// Handles both `Error(string)` and `Panic(uint256)` formats according to
/// Solidity's revert conventions.
///
/// See https://docs.soliditylang.org/en/latest/control-structures.html#panic-via-assert-and-error-via-require
pub(crate) fn parse_eth_revert(data: &[u8]) -> String {
    // If it's not long enough to contain an ABI encoded response, return immediately.
    if data.len() < EVM_FUNC_SELECTOR_LENGTH + EVM_WORD_LENGTH {
        return hex::encode_prefixed(data);
    }

    // Extract function selector (first 4 bytes)
    let selector = data
        .get(..EVM_FUNC_SELECTOR_LENGTH)
        .expect("checked data length >= 4");

    match selector {
        selector if selector == PANIC_FUNCTION_SELECTOR.as_slice() => parse_panic_revert(data),
        selector if selector == ERROR_FUNCTION_SELECTOR.as_slice() => parse_error_revert(data),
        _ => hex::encode_prefixed(data),
    }
}

fn parse_error_revert(data: &[u8]) -> String {
    let fallback = || hex::encode_prefixed(data);

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
    let fallback = || hex::encode_prefixed(data);

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
            .unwrap_or_else(|| format!("Panic(0x{code:x})")))
    })();

    parse_result.unwrap_or_else(|_| fallback())
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    #[test]
    fn eth_storage_at_matches_evm_storage() {
        use crate::db::MemoryDB;
        use crate::networks::ACTOR_BUNDLES_METADATA;
        use crate::shim::econ::TokenAmount;
        use crate::shim::machine::BuiltinActor;
        use crate::utils::db::CborStoreExt as _;

        fn word(n: u8) -> [u8; EVM_WORD_LENGTH] {
            let mut w = [0; EVM_WORD_LENGTH];
            w[EVM_WORD_LENGTH - 1] = n;
            w
        }
        let store = MemoryDB::default();
        let zero = word(0);
        // Newest bundled EVM actor CID, matching the version `default_latest_version` builds.
        let evm_code_cid = ACTOR_BUNDLES_METADATA
            .values()
            .filter_map(|bundle| {
                Some((
                    bundle.actor_major_version().ok()?,
                    bundle.manifest.get(BuiltinActor::EVM).ok()?,
                ))
            })
            .max_by_key(|&(version, _)| version)
            .expect("bundled EVM actor code CID")
            .1;
        let evm_actor = |state: &evm::State| {
            let state_cid = store.put_cbor_default(state).unwrap();
            ActorState::new(evm_code_cid, state_cid, TokenAmount::default(), 0, None)
        };

        // A non-EVM actor has no EVM storage: reads as a zero word.
        let non_evm = ActorState::new(
            Cid::default(),
            Cid::default(),
            TokenAmount::default(),
            0,
            None,
        );
        assert_eq!(non_evm.eth_storage_at(&store, &zero).unwrap(), zero);

        // A live contract returns the stored slot value, and a zero word for an unset slot.
        let mut slots = EvmStorageKamt::new_with_config(&store, evm_kamt_config());
        slots.set(U256::from(5u64), U256::from(42u64)).unwrap();
        let contract_state = slots.flush().unwrap();
        let with_tombstone = |tombstone| {
            evm_actor(&evm::State::default_latest_version(
                Cid::default(),
                [0; 32],
                contract_state,
                None,
                0,
                tombstone,
            ))
        };
        let alive = with_tombstone(None);
        assert_eq!(alive.eth_storage_at(&store, &word(5)).unwrap(), word(42));
        assert_eq!(alive.eth_storage_at(&store, &word(6)).unwrap(), zero);

        // A dead (tombstoned) contract reads a zero word even though the same slots are populated.
        let dead = with_tombstone(Some(evm::Tombstone {
            origin: 100,
            nonce: 1,
        }));
        assert_eq!(dead.eth_storage_at(&store, &word(5)).unwrap(), zero);
    }

    fn create_error_data(msg: &str) -> Vec<u8> {
        let mut encoded = Vec::new();

        // Step 1: Add function selector (keccak256("Error(string)") first 4 bytes)
        encoded.extend_from_slice(&[0x08, 0xc3, 0x79, 0xa0]);

        // Add offset to string data (32 bytes, value = 32)
        // This points to where the string length is stored
        let mut offset_bytes = [0u8; 32];
        offset_bytes[24..32].copy_from_slice(&32u64.to_be_bytes());
        encoded.extend_from_slice(&offset_bytes);

        // Add string length (32 bytes)
        let mut length_bytes = [0u8; 32];
        length_bytes[24..32].copy_from_slice(&(msg.len() as u64).to_be_bytes());
        encoded.extend_from_slice(&length_bytes);

        // Add string data
        encoded.extend_from_slice(msg.as_bytes());

        // Pad to 32-byte boundary
        let padding_needed = (32 - (msg.len() % 32)) % 32;
        encoded.extend_from_slice(&vec![0; padding_needed]);

        encoded
    }

    fn create_panic_data(code: u64) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&PANIC_FUNCTION_SELECTOR);

        // Add padding (24 bytes) + code (32 bytes)
        data.extend_from_slice(&[0; 24]);
        data.extend_from_slice(&code.to_be_bytes());
        data
    }

    #[test]
    fn test_all_valid_parse_panic_revert() {
        for (code, msg) in PANIC_ERROR_CODES.iter() {
            let data = create_panic_data(*code);
            assert_eq!(parse_panic_revert(&data), msg.to_string());
        }
    }

    #[test]
    fn test_all_valid_hex_parse_error_revert() {
        let panic_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000000")
                .unwrap();
        assert_eq!(parse_panic_revert(&panic_data), "Panic()");

        let assert_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        assert_eq!(parse_panic_revert(&assert_data), "Assert()");

        let arithmetic_overflow_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000011")
                .unwrap();
        assert_eq!(
            parse_panic_revert(&arithmetic_overflow_data),
            "ArithmeticOverflow()"
        );

        let divide_by_zero_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000012")
                .unwrap();
        assert_eq!(parse_panic_revert(&divide_by_zero_data), "DivideByZero()");

        let invalid_enum_variant_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000021")
                .unwrap();
        assert_eq!(
            parse_panic_revert(&invalid_enum_variant_data),
            "InvalidEnumVariant()"
        );

        let invalid_storage_array_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000022")
                .unwrap();
        assert_eq!(
            parse_panic_revert(&invalid_storage_array_data),
            "InvalidStorageArray()"
        );

        let pop_empty_array_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000031")
                .unwrap();
        assert_eq!(parse_panic_revert(&pop_empty_array_data), "PopEmptyArray()");

        let array_index_out_of_bounds_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000032")
                .unwrap();
        assert_eq!(
            parse_panic_revert(&array_index_out_of_bounds_data),
            "ArrayIndexOutOfBounds()"
        );

        let out_of_memory_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000041")
                .unwrap();
        assert_eq!(parse_panic_revert(&out_of_memory_data), "OutOfMemory()");

        let call_uninitialized_data =
            hex::decode("4e487b710000000000000000000000000000000000000000000000000000000000000051")
                .unwrap();
        assert_eq!(
            parse_panic_revert(&call_uninitialized_data),
            "CalledUninitializedFunction()"
        );
    }

    #[test]
    fn test_parse_error_revert() {
        let err_msg = "Not enough Ether provided";
        let error_data = create_error_data(err_msg);
        assert_eq!(parse_error_revert(&error_data), format!("Error({err_msg})"));

        // ABI-encoded Error("Hello World")
        let err_data = hex::decode(
            "\
            08c379a0\
            0000000000000000000000000000000000000000000000000000000000000020\
            000000000000000000000000000000000000000000000000000000000000000b\
            48656c6c6f20576f726c64000000000000000000000000000000000000000000\
            ",
        )
        .unwrap();
        assert_eq!(parse_error_revert(&err_data), "Error(Hello World)");

        // ERC20 insufficient balance
        let insufficient = hex::decode(
            "08c379a0\
                0000000000000000000000000000000000000000000000000000000000000020\
                0000000000000000000000000000000000000000000000000000000000000026\
                45524332303a207472616e7366657220616d6f756e7420657863656564732062\
                616c616e63650000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        assert_eq!(
            parse_eth_revert(&insufficient),
            "Error(ERC20: transfer amount exceeds balance)"
        );
    }

    #[test]
    fn test_parse_eth_revert_main_function() {
        // Test normal Error case
        let message = "Transaction failed";
        let data = create_error_data(message);
        assert_eq!(parse_eth_revert(&data), format!("Error({message})"));

        // Test normal Panic case
        let panic_data = create_panic_data(0x01); // Assert()
        assert_eq!(parse_eth_revert(&panic_data), "Assert()");

        // Test data too short for any revert reason
        let short_data = vec![0x1, 0x2, 0x3];
        assert_eq!(
            parse_eth_revert(&short_data),
            format!("0x{}", hex::encode(&short_data))
        );

        // Test unknown function selector
        let mut unknown_selector = vec![0; EVM_FUNC_SELECTOR_LENGTH + EVM_WORD_LENGTH];
        unknown_selector[0] = 0xAA;
        unknown_selector[1] = 0xBB;
        unknown_selector[2] = 0xCC;
        unknown_selector[3] = 0xDD;
        assert_eq!(
            parse_eth_revert(&unknown_selector),
            format!("0x{}", hex::encode(&unknown_selector))
        );
    }

    #[test]
    fn test_parse_error_revert_special_cases() {
        // Test with empty error message
        let data = create_error_data("");
        assert_eq!(parse_error_revert(&data), "Error()");

        // Test with special characters
        let special = "Error message with special chars: !@#$%6^&*()_+{}|:<>!?";
        let data = create_error_data(special);
        assert_eq!(parse_error_revert(&data), format!("Error({special})"));

        // Test with Unicode characters
        let unicode = "Error with Unicode: 你好世界";
        let data = create_error_data(unicode);
        assert_eq!(parse_error_revert(&data), format!("Error({unicode})"));

        // Test with invalid offset (points outside data)
        let mut invalid_offset = create_error_data("Test");
        // Modify offset to point outside available data
        invalid_offset
            .iter_mut()
            .skip(24)
            .take(8)
            .for_each(|byte| *byte = 0xFF);
        assert_eq!(
            parse_error_revert(&invalid_offset),
            format!("0x{}", hex::encode(&invalid_offset))
        );

        // Test with invalid length (exceeds available data)
        let mut invalid_length = create_error_data("Test");
        // Set offset to valid 32, but make length too large
        invalid_length
            .iter_mut()
            .skip(32 + 24)
            .take(8)
            .for_each(|byte| *byte = 0xFF);
        assert_eq!(
            parse_error_revert(&invalid_length),
            format!("0x{}", hex::encode(&invalid_length))
        );

        // Test with truncated data (not enough for string data)
        let truncated = create_error_data("Test");
        let truncated = &truncated[0..70]; // Cut off after length field
        assert_eq!(
            parse_error_revert(truncated),
            format!("0x{}", hex::encode(truncated))
        );

        // Test with invalid UTF-8 in the string
        let mut invalid_utf8 = create_error_data("Test string");
        // Insert invalid UTF-8 sequence
        let string_start = 32 + 32;
        invalid_utf8[string_start + 2] = 0xFF;
        assert_eq!(
            parse_error_revert(&invalid_utf8),
            format!("0x{}", hex::encode(&invalid_utf8))
        );
    }

    #[test]
    fn test_eth_revert_boundary_conditions() {
        // Test with exactly minimum size data
        let min_size = vec![0; EVM_FUNC_SELECTOR_LENGTH + EVM_WORD_LENGTH];
        assert_eq!(
            parse_eth_revert(&min_size),
            format!("0x{}", hex::encode(&min_size))
        );

        // Test with exactly one byte less than minimum
        let too_small = vec![0; EVM_FUNC_SELECTOR_LENGTH + EVM_WORD_LENGTH - 1];
        assert_eq!(
            parse_eth_revert(&too_small),
            format!("0x{}", hex::encode(&too_small))
        );
    }

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
        let encoded = cbor_bytes(vec![1]);

        let result = decode_payload(&encoded, DAG_CBOR);
        assert_eq!(result.unwrap(), EthBytes(vec![1]));

        // regular cbor also works
        let result = decode_payload(&encoded, CBOR);
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

    fn cbor_bytes(inner: Vec<u8>) -> RawBytes {
        RawBytes::new(cbor4ii::serde::to_vec(Vec::new(), &Value::Bytes(inner)).unwrap())
    }

    // Undecodable input is the expected non-Ethereum case; Lotus returns empty data and reason.
    #[rstest]
    #[case(RawBytes::new(vec![0x18]), vec![], "")]
    #[case(cbor_bytes(vec![]), vec![], "none")]
    #[case(
        cbor_bytes(create_error_data("boom")),
        create_error_data("boom"),
        "Error(boom)"
    )]
    #[case(
        cbor_bytes(create_panic_data(0x01)),
        create_panic_data(0x01),
        "Assert()"
    )]
    // Too short for a selector+word, so the reason is the raw hex passthrough.
    #[case(cbor_bytes(vec![0xde, 0xad, 0xbe, 0xef]), vec![0xde, 0xad, 0xbe, 0xef], "0xdeadbeef")]
    fn test_decode_revert_reason(
        #[case] input: RawBytes,
        #[case] expected_data: Vec<u8>,
        #[case] expected_reason: &str,
    ) {
        let (data, reason) = decode_revert_reason(input);
        assert_eq!(data, expected_data);
        assert_eq!(reason, expected_reason);
    }
}

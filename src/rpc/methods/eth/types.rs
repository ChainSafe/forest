// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use libipld::error::SerdeError;
use serde::de::{value::StringDeserializer, IntoDeserializer};

pub const METHOD_GET_BYTE_CODE: u64 = 3;
pub const METHOD_GET_STORAGE_AT: u64 = 5;

#[derive(
    PartialEq,
    Debug,
    Deserialize,
    Serialize,
    Default,
    Clone,
    JsonSchema,
    derive_more::From,
    derive_more::Into,
)]
pub struct EthBytes(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_vec_bytes")]
    pub Vec<u8>,
);
lotus_json_with_self!(EthBytes);

impl From<RawBytes> for EthBytes {
    fn from(value: RawBytes) -> Self {
        Self(value.into())
    }
}

impl FromStr for EthBytes {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let deserializer: StringDeserializer<SerdeError> = String::from_str(s)?.into_deserializer();
        let bytes = crate::lotus_json::hexify_vec_bytes::deserialize(deserializer)?;
        Ok(Self(bytes))
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GetBytecodeReturn(pub Option<Cid>);

const GET_STORAGE_AT_PARAMS_ARRAY_LENGTH: usize = 32;

#[derive(Debug, Clone)]
pub struct GetStorageAtParams(pub [u8; GET_STORAGE_AT_PARAMS_ARRAY_LENGTH]);

impl GetStorageAtParams {
    pub fn new(position: Vec<u8>) -> anyhow::Result<Self> {
        if position.len() > GET_STORAGE_AT_PARAMS_ARRAY_LENGTH {
            anyhow::bail!("supplied storage key is too long");
        }
        let mut bytes = [0; GET_STORAGE_AT_PARAMS_ARRAY_LENGTH];
        bytes
            .get_mut(GET_STORAGE_AT_PARAMS_ARRAY_LENGTH.saturating_sub(position.len())..)
            .expect("Infallible")
            .copy_from_slice(&position);
        Ok(Self(bytes))
    }

    pub fn serialize_params(&self) -> anyhow::Result<Vec<u8>> {
        const LENGTH_BUF_GET_STORAGE_AT_PARAMS: u8 = 129;
        let mut encoded = fvm_ipld_encoding::to_vec(&RawBytes::new(self.0.to_vec()))?;
        encoded.insert(0, LENGTH_BUF_GET_STORAGE_AT_PARAMS);
        Ok(encoded)
    }
}

#[derive(
    PartialEq,
    Debug,
    Deserialize,
    Serialize,
    Default,
    Clone,
    JsonSchema,
    derive_more::From,
    derive_more::Into,
)]
pub struct EthAddress(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_bytes")]
    pub ethereum_types::Address,
);
lotus_json_with_self!(EthAddress);

impl EthAddress {
    pub fn to_filecoin_address(&self) -> anyhow::Result<FilecoinAddress> {
        if self.is_masked_id() {
            const PREFIX_LEN: usize = MASKED_ID_PREFIX.len();
            // This is a masked ID address.
            let arr = self.0.as_fixed_bytes();
            let mut bytes = [0; 8];
            bytes.copy_from_slice(&arr[PREFIX_LEN..]);
            Ok(FilecoinAddress::new_id(u64::from_be_bytes(bytes)))
        } else {
            // Otherwise, translate the address into an address controlled by the
            // Ethereum Address Manager.
            Ok(FilecoinAddress::new_delegated(
                FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()?,
                self.0.as_bytes(),
            )?)
        }
    }

    // See https://github.com/filecoin-project/lotus/blob/v1.26.2/chain/types/ethtypes/eth_types.go#L347-L375 for reference implementation
    pub fn from_filecoin_address(addr: &FilecoinAddress) -> anyhow::Result<Self> {
        match addr.protocol() {
            Protocol::ID => Ok(Self::from_actor_id(addr.id()?)),
            Protocol::Delegated => {
                let payload = addr.payload();
                let result: Result<DelegatedAddress, _> = payload.try_into();
                if let Ok(f4_addr) = result {
                    let namespace = f4_addr.namespace();
                    if namespace != FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()? {
                        bail!("invalid address {addr}");
                    }
                    let eth_addr: EthAddress = f4_addr.subaddress().try_into()?;
                    if eth_addr.is_masked_id() {
                        bail!(
                            "f410f addresses cannot embed masked-ID payloads: {}",
                            eth_addr.0
                        );
                    }
                    return Ok(eth_addr);
                }
                bail!("invalid delegated address namespace in: {addr}")
            }
            _ => {
                bail!("invalid address {addr}");
            }
        }
    }

    pub fn is_masked_id(&self) -> bool {
        self.0.as_bytes().starts_with(&MASKED_ID_PREFIX)
    }

    pub fn from_actor_id(id: u64) -> Self {
        let pfx = MASKED_ID_PREFIX;
        let arr = id.to_be_bytes();
        let payload = [
            pfx[0], pfx[1], pfx[2], pfx[3], pfx[4], pfx[5], pfx[6], pfx[7], //
            pfx[8], pfx[9], pfx[10], pfx[11], //
            arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
        ];

        Self(ethereum_types::H160(payload))
    }
}

impl FromStr for EthAddress {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(EthAddress(
            ethereum_types::Address::from_str(s).map_err(|e| anyhow::anyhow!("{e}"))?,
        ))
    }
}

impl TryFrom<&[u8]> for EthAddress {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != ADDRESS_LENGTH {
            bail!("cannot parse bytes into an Ethereum address: incorrect input length")
        }
        let mut payload = ethereum_types::H160::default();
        payload.as_bytes_mut().copy_from_slice(value);
        Ok(EthAddress(payload))
    }
}

impl TryFrom<&FilecoinAddress> for EthAddress {
    type Error = anyhow::Error;

    fn try_from(value: &FilecoinAddress) -> Result<Self, Self::Error> {
        Self::from_filecoin_address(value)
    }
}

impl TryFrom<FilecoinAddress> for EthAddress {
    type Error = anyhow::Error;

    fn try_from(value: FilecoinAddress) -> Result<Self, Self::Error> {
        Self::from_filecoin_address(&value)
    }
}

impl From<[u8; 20]> for EthAddress {
    fn from(value: [u8; 20]) -> Self {
        Self(ethereum_types::H160(value))
    }
}

impl TryFrom<&EthAddress> for FilecoinAddress {
    type Error = anyhow::Error;

    fn try_from(value: &EthAddress) -> Result<Self, Self::Error> {
        value.to_filecoin_address()
    }
}

impl TryFrom<EthAddress> for FilecoinAddress {
    type Error = anyhow::Error;

    fn try_from(value: EthAddress) -> Result<Self, Self::Error> {
        value.to_filecoin_address()
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BlockNumberOrPredefined {
    #[schemars(with = "String")]
    PredefinedBlock(Predefined),
    BlockNumber(Int64),
}
lotus_json_with_self!(BlockNumberOrPredefined);

impl From<BlockNumberOrPredefined> for BlockNumberOrHash {
    fn from(value: BlockNumberOrPredefined) -> Self {
        match value {
            BlockNumberOrPredefined::PredefinedBlock(v) => BlockNumberOrHash::PredefinedBlock(v),
            BlockNumberOrPredefined::BlockNumber(v) => BlockNumberOrHash::BlockNumber(v),
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthFeeHistoryResult {
    pub oldest_block: Uint64,
    pub base_fee_per_gas: Vec<EthBigInt>,
    pub gas_used_ratio: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward: Option<Vec<Vec<EthBigInt>>>,
}
lotus_json_with_self!(EthFeeHistoryResult);

#[derive(PartialEq, Debug, Clone)]
pub struct GasReward {
    pub gas_used: u64,
    pub premium: TokenAmount,
}

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthCallMessage {
    pub from: Option<EthAddress>,
    pub to: Option<EthAddress>,
    pub gas: Uint64,
    pub gas_price: EthBigInt,
    pub value: EthBigInt,
    pub data: EthBytes,
}
lotus_json_with_self!(EthCallMessage);

impl EthCallMessage {
    pub fn convert_data_to_message_params(data: EthBytes) -> anyhow::Result<RawBytes> {
        if data.0.is_empty() {
            Ok(RawBytes::new(data.0))
        } else {
            Ok(RawBytes::new(fvm_ipld_encoding::to_vec(&RawBytes::new(
                data.0,
            ))?))
        }
    }
}

impl TryFrom<EthCallMessage> for Message {
    type Error = anyhow::Error;
    fn try_from(tx: EthCallMessage) -> Result<Self, Self::Error> {
        let from = match &tx.from {
            Some(addr) if addr != &EthAddress::default() => {
                // The from address must be translatable to an f4 address.
                let from = addr.to_filecoin_address()?;
                if from.protocol() != Protocol::Delegated {
                    anyhow::bail!("expected a class 4 address, got: {}", from.protocol());
                }
                from
            }
            _ => {
                // Send from the filecoin "system" address.
                EthAddress::default().to_filecoin_address()?
            }
        };
        let params = EthCallMessage::convert_data_to_message_params(tx.data)?;
        let (to, method_num) = if let Some(to) = tx.to {
            (
                to.to_filecoin_address()?,
                EVMMethod::InvokeContract as MethodNum,
            )
        } else {
            (
                FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR,
                EAMMethod::CreateExternal as MethodNum,
            )
        };
        Ok(Message {
            from,
            to,
            value: tx.value.0.into(),
            method_num,
            params,
            gas_limit: BLOCK_GAS_LIMIT,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{prelude::BASE64_STANDARD, Engine as _};

    #[test]
    fn get_bytecode_return_roundtrip() {
        let bytes = hex::decode("d82a5827000155a0e40220fa0b7a54007ba2e76d5818b6e60793fb0b8bdbe177995e1b20dcfb6873d69779").unwrap();
        let des: GetBytecodeReturn = fvm_ipld_encoding::from_slice(&bytes).unwrap();
        assert_eq!(
            des.0.unwrap().to_string(),
            "bafk2bzaced5aw6suab52fz3nlamlnzqhsp5qxc634f3zsxq3edopw2dt22lxs"
        );
        let ser = fvm_ipld_encoding::to_vec(&des).unwrap();
        assert_eq!(ser, bytes);
    }

    #[test]
    fn get_storage_at_params() {
        let param = GetStorageAtParams::new(vec![0xa]).unwrap();
        assert_eq!(
            &hex::encode(param.serialize_params().unwrap()),
            "815820000000000000000000000000000000000000000000000000000000000000000a"
        );
    }

    #[test]
    fn test_convert_data_to_message_params_empty() {
        let data = EthBytes(vec![]);
        let params = EthCallMessage::convert_data_to_message_params(data).unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn test_convert_data_to_message_params() {
        let data = EthBytes(BASE64_STANDARD.decode("RHt4g0E=").unwrap());
        let params = EthCallMessage::convert_data_to_message_params(data).unwrap();
        assert_eq!(BASE64_STANDARD.encode(&*params).as_str(), "RUR7eINB");
    }
}

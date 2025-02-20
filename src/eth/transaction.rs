// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{derive_eip_155_chain_id, validate_eip155_chain_id};
use crate::eth::{LEGACY_V_VALUE_27, LEGACY_V_VALUE_28};
use crate::shim::crypto::Signature;
use crate::shim::fvm_shared_latest;
use anyhow::{bail, ensure, Context};
use bytes::BytesMut;
use cbor4ii::core::{dec::Decode as _, utils::SliceReader, Value};
use fvm_shared4::METHOD_CONSTRUCTOR;
use num::{bigint::Sign, BigInt, Signed as _};
use num_derive::FromPrimitive;
use num_traits::cast::ToPrimitive;
use rlp::Rlp;

use crate::{
    message::{Message as _, SignedMessage},
    rpc::eth::types::EthAddress,
    shim::{address::Address, crypto::SignatureType, message::Message, version::NetworkVersion},
};

use super::{
    eip_1559_transaction::{EthEip1559TxArgs, EthEip1559TxArgsBuilder, EIP_1559_SIG_LEN},
    eip_155_transaction::{
        calc_valid_eip155_sig_len, EthLegacyEip155TxArgs, EthLegacyEip155TxArgsBuilder,
        EIP_155_SIG_PREFIX,
    },
    homestead_transaction::{
        EthLegacyHomesteadTxArgs, EthLegacyHomesteadTxArgsBuilder, HOMESTEAD_SIG_LEN,
        HOMESTEAD_SIG_PREFIX,
    },
    EthChainId, EIP_1559_TX_TYPE, EIP_2930_TX_TYPE,
};
// As per `ref-fvm`, which hardcodes it as well.
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum EAMMethod {
    Constructor = METHOD_CONSTRUCTOR,
    Create = 2,
    Create2 = 3,
    CreateExternal = 4,
}

#[derive(FromPrimitive)]
#[repr(u64)]
pub enum EVMMethod {
    Constructor = METHOD_CONSTRUCTOR,
    Resurrect = 2,
    GetBytecode = 3,
    GetBytecodeHash = 4,
    GetStorageAt = 5,
    InvokeContractDelegate = 6,
    // As per `ref-fvm`:
    // it is very unfortunate but the hasher creates a circular dependency, so we use the raw
    // number.
    // InvokeContract = frc42_dispatch::method_hash!("InvokeEVM"),
    InvokeContract = 3844450837,
}

/// Ethereum transaction which can be of different types.
/// The currently supported types are defined in [FIP-0091](https://github.com/filecoin-project/FIPs/blob/020bcb412ee20a2879b4a710337959c51b938d3b/FIPS/fip-0091.md).
pub enum EthTx {
    Homestead(Box<EthLegacyHomesteadTxArgs>),
    Eip1559(Box<EthEip1559TxArgs>),
    Eip155(Box<EthLegacyEip155TxArgs>),
}

impl EthTx {
    /// Creates an Ethereum transaction from a signed Filecoin message.
    /// The transaction type is determined based on the signature, as defined in FIP-0091.
    pub fn from_signed_message(
        eth_chain_id: EthChainId,
        msg: &SignedMessage,
    ) -> anyhow::Result<Self> {
        Self::ensure_signed_message_valid(msg)?;

        // now we need to determine the transaction type based on the signature length
        let sig_len = msg.signature().bytes().len();

        // valid signature lengths are based on the chain ID, so we need to calculate it. This
        // shouldn't be a resource-intensive operation, but if it becomes one, we can do some
        // memoization.
        let valid_eip_155_signature_lengths = calc_valid_eip155_sig_len(eth_chain_id);

        let tx: Self = if sig_len == EIP_1559_SIG_LEN {
            let args = EthEip1559TxArgsBuilder::default()
                .chain_id(eth_chain_id)
                .unsigned_message(msg.message())?
                .build()?
                .with_signature(msg.signature())?;
            EthTx::Eip1559(Box::new(args))
        } else if sig_len == HOMESTEAD_SIG_LEN
            || sig_len == valid_eip_155_signature_lengths.0 as usize
            || sig_len == valid_eip_155_signature_lengths.1 as usize
        {
            // process based on the first byte of the signature
            match *msg.signature().bytes().first().expect("infallible") {
                HOMESTEAD_SIG_PREFIX => {
                    let args = EthLegacyHomesteadTxArgsBuilder::default()
                        .unsigned_message(msg.message())?
                        .build()?
                        .with_signature(msg.signature())?;
                    EthTx::Homestead(Box::new(args))
                }
                EIP_155_SIG_PREFIX => {
                    let args = EthLegacyEip155TxArgsBuilder::default()
                        .chain_id(eth_chain_id)
                        .unsigned_message(msg.message())?
                        .build()?
                        .with_signature(msg.signature())?;
                    EthTx::Eip155(Box::new(args))
                }
                _ => bail!("unsupported signature prefix"),
            }
        } else {
            bail!("unsupported signature length: {sig_len}");
        };

        Ok(tx)
    }

    pub fn eth_hash(&self) -> anyhow::Result<keccak_hash::H256> {
        Ok(keccak_hash::keccak(self.rlp_signed_message()?))
    }

    pub fn get_signed_message(&self, eth_chain_id: EthChainId) -> anyhow::Result<SignedMessage> {
        let from = self.sender(eth_chain_id)?;
        let msg = match self {
            Self::Homestead(tx) => (*tx).get_signed_message(from)?,
            Self::Eip1559(tx) => (*tx).get_signed_message(from, eth_chain_id)?,
            Self::Eip155(tx) => (*tx).get_signed_message(from, eth_chain_id)?,
        };
        Ok(msg)
    }

    pub fn get_unsigned_message(
        &self,
        from: Address,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<Message> {
        let msg = match self {
            Self::Homestead(tx) => (*tx).get_unsigned_message(from)?,
            Self::Eip1559(tx) => (*tx).get_unsigned_message(from, eth_chain_id)?,
            Self::Eip155(tx) => (*tx).get_unsigned_message(from, eth_chain_id)?,
        };
        Ok(msg)
    }

    pub fn rlp_unsigned_message(&self, eth_chain_id: EthChainId) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Homestead(tx) => (*tx).rlp_unsigned_message(),
            Self::Eip1559(tx) => (*tx).rlp_unsigned_message(),
            Self::Eip155(tx) => (*tx).rlp_unsigned_message(eth_chain_id),
        }
    }

    fn rlp_signed_message(&self) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Homestead(tx) => (*tx).rlp_signed_message(),
            Self::Eip1559(tx) => (*tx).rlp_signed_message(),
            Self::Eip155(tx) => (*tx).rlp_signed_message(),
        }
    }

    fn signature(&self, eth_chain_id: EthChainId) -> anyhow::Result<Signature> {
        match self {
            Self::Homestead(tx) => (*tx).signature(),
            Self::Eip1559(tx) => (*tx).signature(),
            Self::Eip155(tx) => (*tx).signature(eth_chain_id),
        }
    }

    pub fn to_verifiable_signature(
        &self,
        sig: Vec<u8>,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Homestead(tx) => (*tx).to_verifiable_signature(sig),
            Self::Eip1559(tx) => (*tx).to_verifiable_signature(sig),
            Self::Eip155(tx) => (*tx).to_verifiable_signature(sig, eth_chain_id),
        }
    }

    /// Checks if the transaction is EIP-1559
    pub fn is_eip1559(&self) -> bool {
        matches!(self, EthTx::Eip1559(_))
    }

    /// Validates that the signed Filecoin message is a valid Ethereum transaction.
    /// Note: only basic checks are done. The signature and payload are not verified.
    fn ensure_signed_message_valid(msg: &SignedMessage) -> anyhow::Result<()> {
        ensure!(
            msg.signature().signature_type() == SignatureType::Delegated,
            "Signature is not delegated type"
        );

        ensure!(
            msg.message().version == 0,
            "unsupported msg version: {}",
            msg.message().version
        );

        EthAddress::from_filecoin_address(&msg.from())?;

        Ok(())
    }

    fn sender(&self, eth_chain_id: EthChainId) -> anyhow::Result<Address> {
        let hash = keccak_hash::keccak(self.rlp_unsigned_message(eth_chain_id)?);
        let sig = self.signature(eth_chain_id)?;
        let sig_data = self.to_verifiable_signature(sig.bytes().to_vec(), eth_chain_id)?[..]
            .try_into()
            .expect("Incorrect signature length");
        let pubkey =
            fvm_shared_latest::crypto::signature::ops::recover_secp_public_key(&hash.0, &sig_data)?;
        let eth_addr = EthAddress::eth_address_from_pub_key(&pubkey.serialize())?;
        eth_addr.to_filecoin_address()
    }
}

/// Checks if a signed Filecoin message is valid for sending to Ethereum.
pub fn is_valid_eth_tx_for_sending(
    eth_chain_id: EthChainId,
    network_version: NetworkVersion,
    message: &SignedMessage,
) -> bool {
    let eth_tx = EthTx::from_signed_message(eth_chain_id, message);

    if let Ok(eth_tx) = eth_tx {
        // EIP-1559 transactions are valid for all network versions.
        // Legacy transactions are only valid for network versions >= V23.
        network_version >= NetworkVersion::V23 || eth_tx.is_eip1559()
    } else {
        false
    }
}

/// Extracts the Ethereum transaction parameters and recipient from a Filecoin message.
pub fn get_eth_params_and_recipient(
    msg: &Message,
) -> anyhow::Result<(Vec<u8>, Option<EthAddress>)> {
    let mut to = None;
    let mut params = vec![];

    ensure!(msg.version == 0, "unsupported msg version: {}", msg.version);

    if !msg.params().bytes().is_empty() {
        let mut reader = SliceReader::new(msg.params().bytes());
        match Value::decode(&mut reader) {
            Ok(Value::Bytes(bytes)) => params = bytes,
            _ => bail!("failed to read params byte array"),
        }
    }

    if msg.to == Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR {
        if msg.method_num() != EAMMethod::CreateExternal as u64 {
            bail!("unsupported EAM method");
        }
    } else if msg.method_num() == EVMMethod::InvokeContract as u64 {
        let addr = EthAddress::from_filecoin_address(&msg.to)?;
        to = Some(addr);
    } else {
        bail!(
            "invalid methodnum {}: only allowed method is InvokeContract({})",
            msg.method_num(),
            EVMMethod::InvokeContract as u64
        );
    }

    Ok((params, to))
}

pub fn format_u64(value: u64) -> BytesMut {
    if value != 0 {
        let i = (value.leading_zeros() / 8) as usize;
        let bytes = value.to_be_bytes();
        // `leading_zeros` for a positive `u64` returns a number in the range [1-63]
        // `i` is in the range [1-7], and `bytes` is an array of size 8
        // therefore, getting the slice from `i` to end should never fail
        bytes.get(i..).expect("failed to get slice").into()
    } else {
        // If all bytes are zero, return an empty slice
        BytesMut::new()
    }
}

pub fn format_bigint(value: &BigInt) -> anyhow::Result<BytesMut> {
    Ok(if value.is_positive() {
        BytesMut::from_iter(value.to_bytes_be().1.iter())
    } else {
        if value.is_negative() {
            bail!("can't format a negative number");
        }
        // If all bytes are zero, return an empty slice
        BytesMut::new()
    })
}

pub fn format_address(value: &Option<EthAddress>) -> BytesMut {
    if let Some(addr) = value {
        addr.0.as_bytes().into()
    } else {
        BytesMut::new()
    }
}

/// Pads data with leading zeros to the specified length
pub fn pad_leading_zeros(data: Vec<u8>, length: usize) -> Vec<u8> {
    if data.len() >= length {
        return data;
    }
    let mut zeros = vec![0; length - data.len()];
    zeros.extend(data);
    zeros
}

/// Parses raw bytes into an eth transaction
pub fn parse_eth_transaction(data: &[u8]) -> anyhow::Result<EthTx> {
    ensure!(!data.is_empty(), "eth transaction data is empty");

    match data.first() {
        Some(&EIP_2930_TX_TYPE) => {
            // EIP-2930
            Err(anyhow::anyhow!("EIP-2930 transaction is not supported"))
        }
        Some(&EIP_1559_TX_TYPE) => {
            parse_eip1559_tx(data).context("Failed to parse EIP-1559 transaction")
        }
        Some(tx_type) if *tx_type > 0x7f => parse_legacy_tx(data)
            .map_err(|err| anyhow::anyhow!("failed to parse legacy transaction: {}", err)),
        _ => Err(anyhow::anyhow!("unsupported transaction type")),
    }
}

fn parse_eip1559_tx(data: &[u8]) -> anyhow::Result<EthTx> {
    // Decode RLP data, skipping the first byte (EIP_1559_TX_TYPE)
    let decoded = Rlp::new(data.get(1..).context("failed to get range of values")?);
    ensure!(
        decoded.item_count()? == 12,
        "not an EIP-1559 transaction: should have 12 elements in the Rlp list"
    );

    let chain_id = decoded.at(0)?.as_val::<u64>()?;
    let nonce = decoded.at(1)?.as_val::<u64>()?;
    let max_priority_fee_per_gas = BigInt::from_bytes_be(Sign::Plus, decoded.at(2)?.data()?);
    let max_fee_per_gas = BigInt::from_bytes_be(Sign::Plus, decoded.at(3)?.data()?);
    let gas_limit = decoded.at(4)?.as_val::<u64>()?;

    let addr_data = decoded.at(5)?.data()?;
    let to = (!addr_data.is_empty())
        .then(|| EthAddress::try_from(addr_data))
        .transpose()?;

    let value = BigInt::from_bytes_be(Sign::Plus, decoded.at(6)?.data()?);
    let input = decoded.at(7)?.data()?.to_vec();

    // Ensure access list is empty (should be an empty list)
    ensure!(
        decoded.at(8)?.item_count()? == 0,
        "access list should be an empty list"
    );

    let v = BigInt::from_bytes_be(Sign::Plus, decoded.at(9)?.data()?);
    let r = BigInt::from_bytes_be(Sign::Plus, decoded.at(10)?.data()?);
    let s = BigInt::from_bytes_be(Sign::Plus, decoded.at(11)?.data()?);

    // EIP-1559 transactions only support 0 or 1 for v
    ensure!(
        v == BigInt::from(0) || v == BigInt::from(1),
        "EIP-1559 transactions only support 0 or 1 for v"
    );

    // Construct and return the Eth1559TxArgs struct
    let tx_args = EthEip1559TxArgs {
        chain_id,
        nonce,
        to,
        max_priority_fee_per_gas,
        max_fee_per_gas,
        gas_limit,
        value,
        input,
        v,
        r,
        s,
    };

    Ok(EthTx::Eip1559(Box::new(tx_args)))
}

fn parse_legacy_tx(data: &[u8]) -> anyhow::Result<EthTx> {
    // Decode RLP data
    let decoded = Rlp::new(data);
    if decoded.item_count()? != 9 {
        bail!("not a Legacy transaction: should have 9 elements in the rlp list");
    }

    // Parse transaction fields
    let nonce = decoded.at(0)?.as_val::<u64>()?;
    let gas_price = BigInt::from_bytes_be(Sign::Plus, decoded.at(1)?.data()?);
    let gas_limit = decoded.at(2)?.as_val::<u64>()?;

    let addr_data = decoded.at(3)?.data()?;
    let to = (!addr_data.is_empty())
        .then(|| EthAddress::try_from(addr_data))
        .transpose()?;

    let value = BigInt::from_bytes_be(Sign::Plus, decoded.at(4)?.data()?);
    let input = decoded.at(5)?.data()?.to_vec();

    // Parse signature fields
    let v = BigInt::from_bytes_be(Sign::Plus, decoded.at(6)?.data()?);
    let r = BigInt::from_bytes_be(Sign::Plus, decoded.at(7)?.data()?);
    let s = BigInt::from_bytes_be(Sign::Plus, decoded.at(8)?.data()?);

    // Derive chain ID from the 'v' field
    let chain_id = derive_eip_155_chain_id(&v)?
        .to_u64()
        .context("unable to convert derived chain to `u64`")?;

    // Check if the transaction is a legacy Homestead transaction
    if chain_id == 0 {
        // Validate that 'v' is either 27 or 28
        ensure!(
            v == BigInt::from(LEGACY_V_VALUE_27) || v == BigInt::from(LEGACY_V_VALUE_28),
            "legacy homestead transactions only support 27 or 28 for v, got {}",
            v
        );

        let tx_args = EthLegacyHomesteadTxArgs {
            nonce,
            gas_price,
            gas_limit,
            to,
            value,
            input,
            v,
            r,
            s,
        };
        return Ok(EthTx::Homestead(Box::new(tx_args)));
    }

    // For EIP-155 transactions, validate chain ID protection
    validate_eip155_chain_id(chain_id, &v)?;

    Ok(EthTx::Eip155(Box::new(EthLegacyEip155TxArgs {
        chain_id,
        nonce,
        gas_price,
        gas_limit,
        to,
        value,
        input,
        v,
        r,
        s,
    })))
}

#[derive(Debug)]
pub struct MethodInfo {
    pub to: Address,
    pub method: u64,
    pub params: Vec<u8>,
}

/// Retrieves method info
pub fn get_filecoin_method_info(
    recipient: &Option<EthAddress>,
    input: &[u8],
) -> anyhow::Result<MethodInfo> {
    let params = if !input.is_empty() {
        cbor4ii::serde::to_vec(
            Vec::with_capacity(input.len()),
            &Value::Bytes(input.to_vec()),
        )
        .context("failed to encode params")?
    } else {
        Vec::new()
    };

    let (to, method) = match recipient {
        None => {
            // If recipient is None, use Ethereum Address Manager Actor and CreateExternal method
            (
                Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR,
                EAMMethod::CreateExternal as u64,
            )
        }
        Some(recipient) => {
            // Otherwise, use InvokeContract method and convert EthAddress to Filecoin address
            let to = recipient
                .to_filecoin_address()
                .context("failed to convert EthAddress to Filecoin address")?;
            (to, EVMMethod::InvokeContract as u64)
        }
    };

    Ok(MethodInfo { to, method, params })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        networks::{calibnet, mainnet},
        shim::{crypto::Signature, econ::TokenAmount},
    };
    use num::{traits::FromBytes as _, BigInt, Num as _, Zero as _};
    use num_bigint::ToBigUint as _;
    use quickcheck_macros::quickcheck;
    use std::str::FromStr as _;

    const ETH_ADDR_LEN: usize = 20;

    pub fn create_message() -> Message {
        let from = EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa")
            .unwrap()
            .to_filecoin_address()
            .unwrap();

        let to = Address::new_id(1);
        Message {
            version: 0,
            to,
            from,
            value: TokenAmount::from_atto(10),
            gas_fee_cap: TokenAmount::from_atto(11),
            gas_premium: TokenAmount::from_atto(12),
            gas_limit: 13,
            sequence: 14,
            method_num: EVMMethod::InvokeContract as u64,
            params: Default::default(),
        }
    }

    pub fn create_eip_1559_signed_message() -> SignedMessage {
        let mut eip_1559_sig = vec![0u8; EIP_1559_SIG_LEN];
        eip_1559_sig[0] = EIP_155_SIG_PREFIX;

        SignedMessage {
            message: create_message(),
            signature: Signature::new(SignatureType::Delegated, eip_1559_sig),
        }
    }

    pub fn create_homestead_signed_message() -> SignedMessage {
        let mut homestead_sig = vec![0u8; HOMESTEAD_SIG_LEN];
        homestead_sig[0] = HOMESTEAD_SIG_PREFIX;
        homestead_sig[HOMESTEAD_SIG_LEN - 1] = 27;

        SignedMessage {
            message: create_message(),
            signature: Signature::new(SignatureType::Delegated, homestead_sig),
        }
    }

    #[test]
    fn test_ensure_signed_message_valid() {
        let create_empty_delegated_message = || SignedMessage {
            message: create_message(),
            signature: Signature::new(SignatureType::Delegated, vec![]),
        };
        // ok
        let msg = create_empty_delegated_message();
        EthTx::ensure_signed_message_valid(&msg).unwrap();

        // wrong signature type
        let mut msg = create_empty_delegated_message();
        msg.signature = Signature::new(SignatureType::Bls, vec![]);
        assert!(EthTx::ensure_signed_message_valid(&msg).is_err());

        // unsupported version
        let mut msg = create_empty_delegated_message();
        msg.message.version = 1;
        assert!(EthTx::ensure_signed_message_valid(&msg).is_err());

        // invalid delegated address namespace
        let mut msg = create_empty_delegated_message();
        msg.message.from = Address::new_delegated(0x42, &[0xff; ETH_ADDR_LEN]).unwrap();
        assert!(EthTx::ensure_signed_message_valid(&msg).is_err());
    }

    #[test]
    fn test_eth_transaction_from_signed_filecoin_message_valid_eip1559() {
        let msg = create_eip_1559_signed_message();

        let tx = EthTx::from_signed_message(mainnet::ETH_CHAIN_ID, &msg).unwrap();
        if let EthTx::Eip1559(tx) = tx {
            assert_eq!(tx.chain_id, mainnet::ETH_CHAIN_ID);
            assert_eq!(tx.value, msg.message.value.into());
            assert_eq!(tx.max_fee_per_gas, msg.message.gas_fee_cap.into());
            assert_eq!(tx.max_priority_fee_per_gas, msg.message.gas_premium.into());
            assert_eq!(tx.gas_limit, msg.message.gas_limit);
            assert_eq!(tx.nonce, msg.message.sequence);
            assert_eq!(
                tx.to.unwrap(),
                EthAddress::from_filecoin_address(&msg.message.to).unwrap()
            );
            assert!(tx.input.is_empty());
        } else {
            panic!("invalid transaction type");
        }
    }

    #[test]
    fn test_eth_transaction_from_signed_filecoin_message_valid_homestead() {
        let msg = create_homestead_signed_message();
        let tx = EthTx::from_signed_message(mainnet::ETH_CHAIN_ID, &msg).unwrap();
        if let EthTx::Homestead(tx) = tx {
            assert_eq!(tx.value, msg.message.value.into());
            assert_eq!(tx.gas_limit, msg.message.gas_limit);
            assert_eq!(tx.nonce, msg.message.sequence);
            assert_eq!(
                tx.to.unwrap(),
                EthAddress::from_filecoin_address(&msg.message.to).unwrap()
            );
            assert_eq!(tx.gas_price, msg.message.gas_fee_cap.into());
            assert!(tx.input.is_empty());
        } else {
            panic!("invalid transaction type");
        }
    }

    #[test]
    fn test_eth_transaction_from_signed_filecoin_message_valid_eip_155() {
        let eth_chain_id = mainnet::ETH_CHAIN_ID;

        // we need some reverse math here to get the correct V value
        // from which the chain ID is derived.
        let v = (2 * eth_chain_id + 35).to_biguint().unwrap().to_bytes_be();
        let eip_155_sig_len = calc_valid_eip155_sig_len(eth_chain_id).0 as usize;
        let mut eip_155_sig = vec![0u8; eip_155_sig_len as usize - v.len()];
        eip_155_sig[0] = EIP_155_SIG_PREFIX;
        eip_155_sig.extend(v);

        let msg = SignedMessage {
            message: create_message(),
            signature: Signature::new(SignatureType::Delegated, eip_155_sig),
        };

        let tx = EthTx::from_signed_message(mainnet::ETH_CHAIN_ID, &msg).unwrap();
        if let EthTx::Eip155(tx) = tx {
            assert_eq!(tx.value, msg.message.value.into());
            assert_eq!(tx.gas_limit, msg.message.gas_limit);
            assert_eq!(tx.nonce, msg.message.sequence);
            assert_eq!(
                tx.to.unwrap(),
                EthAddress::from_filecoin_address(&msg.message.to).unwrap()
            );
            assert_eq!(tx.gas_price, msg.message.gas_fee_cap.into());
            assert!(tx.input.is_empty());
        } else {
            panic!("invalid transaction type");
        }
    }

    #[test]
    fn test_eth_transaction_from_signed_filecoin_message_empty_signature() {
        let msg = SignedMessage {
            message: create_message(),
            signature: Signature::new(SignatureType::Delegated, vec![]),
        };

        assert!(EthTx::from_signed_message(mainnet::ETH_CHAIN_ID, &msg).is_err());
    }

    #[test]
    fn test_eth_transaction_from_signed_filecoin_message_invalid_signature() {
        let msg = SignedMessage {
            message: create_message(),
            signature: Signature::new(
                SignatureType::Delegated,
                b"Ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn".to_vec(),
            ),
        };

        assert!(EthTx::from_signed_message(mainnet::ETH_CHAIN_ID, &msg).is_err());
    }

    #[test]
    fn test_is_valid_eth_tx_for_sending_eip1559_always_valid() {
        let msg = create_eip_1559_signed_message();
        assert!(is_valid_eth_tx_for_sending(
            mainnet::ETH_CHAIN_ID,
            NetworkVersion::V22,
            &msg
        ));

        assert!(is_valid_eth_tx_for_sending(
            mainnet::ETH_CHAIN_ID,
            NetworkVersion::V23,
            &msg
        ));
    }

    #[test]
    fn test_is_valid_eth_tx_for_sending_legacy_valid_post_nv23() {
        let msg = create_homestead_signed_message();
        assert!(is_valid_eth_tx_for_sending(
            mainnet::ETH_CHAIN_ID,
            NetworkVersion::V23,
            &msg
        ));
    }

    #[test]
    fn test_is_valid_eth_tx_for_sending_legacy_invalid_pre_nv23() {
        let msg = create_homestead_signed_message();
        assert!(!is_valid_eth_tx_for_sending(
            mainnet::ETH_CHAIN_ID,
            NetworkVersion::V22,
            &msg
        ));
    }

    #[test]
    fn test_is_valid_eth_tx_for_sending_invalid_non_delegated() {
        let msg = create_message();
        let msg = SignedMessage {
            message: msg,
            signature: Signature::new_secp256k1(vec![]),
        };
        assert!(!is_valid_eth_tx_for_sending(
            mainnet::ETH_CHAIN_ID,
            NetworkVersion::V22,
            &msg
        ));
        assert!(!is_valid_eth_tx_for_sending(
            mainnet::ETH_CHAIN_ID,
            NetworkVersion::V23,
            &msg
        ));
    }

    #[test]
    fn test_eip_1559() {
        let mut tx_args=EthEip1559TxArgsBuilder::default()
            .chain_id(314159_u64)
            .nonce(486_u64)
            .to(Some(
                ethereum_types::H160::from_str("0xeb4a9cdb9f42d3a503d580a39b6e3736eb21fffd")
                    .unwrap()
                    .into(),
            ))
            .value(BigInt::from(0))
            .max_fee_per_gas(BigInt::from(1500000120))
            .max_priority_fee_per_gas(BigInt::from(1500000000))
            .gas_limit(37442471_u64)
            .input(hex::decode("383487be000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000660d4d120000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000003b6261666b726569656f6f75326d36356276376561786e7767656d7562723675787269696867366474646e6c7a663469616f37686c6e6a6d647372750000000000").unwrap())
            .build()
            .unwrap();
        tx_args.v = BigInt::from_str("1").unwrap();
        tx_args.r = BigInt::from_str(
            "84103132941276310528712440865285269631208564772362393569572880532520338257200",
        )
        .unwrap();
        tx_args.s = BigInt::from_str(
            "7820796778417228639067439047870612492553874254089570360061550763595363987236",
        )
        .unwrap();
        let tx = EthTx::Eip1559(Box::new(tx_args));
        let sig = tx.signature(calibnet::ETH_CHAIN_ID);
        assert!(sig.is_ok());
        assert!(tx
            .to_verifiable_signature(sig.unwrap().bytes().to_vec(), calibnet::ETH_CHAIN_ID)
            .is_ok());
        assert!(tx.rlp_unsigned_message(calibnet::ETH_CHAIN_ID).is_ok());
        assert!(tx.get_signed_message(calibnet::ETH_CHAIN_ID).is_ok());
        let expected_hash = ethereum_types::H256::from_str(
            "0x9f2e70d5737c6b798eccea14895893fb48091ab3c59d0fe95508dc7efdae2e5f",
        )
        .unwrap();
        assert_eq!(expected_hash, tx.eth_hash().unwrap());
    }

    #[test]
    fn test_legacy_eip_155() {
        // https://calibration.filfox.info/en/message/bafy2bzacebazsfc63saveaopjjgsz3yoic3izod4k5wo3pg4fswmpdqny5zlc?t=1
        let mut tx_args = EthLegacyEip155TxArgsBuilder::default()
            .chain_id(314159_u64)
            .nonce(0x4_u64)
            .to(Some(
                ethereum_types::H160::from_str("0xd0fb381fc644cdd5d694d35e1afb445527b9244b")
                    .unwrap()
                    .into(),
            ))
            .value(BigInt::from(0))
            .gas_limit(0x19ca81cc_u64)
            .gas_price(BigInt::from(0x40696))
            .input(hex::decode("d5b3d76d00000000000000000000000000000000000000000000000045466fa6fdcb80000000000000000000000000000000000000000000000000000000002e90edd0000000000000000000000000000000000000000000000000000000000000015180").unwrap())
            .build()
            .unwrap();
        tx_args.v = BigInt::from_str_radix("99681", 16).unwrap();
        tx_args.r = BigInt::from_str_radix(
            "580b1d36c5a8c8c1c550fb45b0a6ff21aaa517be036385541621961b5d873796",
            16,
        )
        .unwrap();
        tx_args.s = BigInt::from_str_radix(
            "55e8447d58d64ebc3038d9882886bbc3b0228c7ac77c71f4e811b97ed3f14b5a",
            16,
        )
        .unwrap();
        let tx = EthTx::Eip155(Box::new(tx_args));
        let sig = tx.signature(calibnet::ETH_CHAIN_ID);
        assert!(sig.is_ok());
        assert!(tx
            .to_verifiable_signature(sig.unwrap().bytes().to_vec(), calibnet::ETH_CHAIN_ID)
            .is_ok());
        assert!(tx.rlp_unsigned_message(calibnet::ETH_CHAIN_ID).is_ok());
        assert!(tx.get_signed_message(calibnet::ETH_CHAIN_ID).is_ok());
        let expected_hash = ethereum_types::H256::from_str(
            "0x3ebc897150feeff6caa1b2e5992e347e8409e9e35fa30f7f5f8fcda3f7c965c7",
        )
        .unwrap();
        assert_eq!(expected_hash, tx.eth_hash().unwrap());
    }

    #[test]
    fn test_legacy_homestead() {
        // https://calibration.filfox.info/en/message/bafy2bzacebazsfc63saveaopjjgsz3yoic3izod4k5wo3pg4fswmpdqny5zlc?t=1
        let mut tx_args = EthLegacyHomesteadTxArgsBuilder::default()
            .nonce(0x4_u64)
            .to(Some(
                ethereum_types::H160::from_str("0xd0fb381fc644cdd5d694d35e1afb445527b9244b")
                    .unwrap()
                    .into(),
            ))
            .value(BigInt::from(0))
            .gas_limit(0x19ca81cc_u64)
            .gas_price(BigInt::from(0x40696))
            .input(hex::decode("d5b3d76d00000000000000000000000000000000000000000000000045466fa6fdcb80000000000000000000000000000000000000000000000000000000002e90edd0000000000000000000000000000000000000000000000000000000000000015180").unwrap())
            .build()
            .unwrap();
        // Note that the `v` value in this test case is invalid for homestead
        // when it's normally assigned in `with_signature`
        tx_args.v = BigInt::from_str_radix("99681", 16).unwrap();
        tx_args.r = BigInt::from_str_radix(
            "580b1d36c5a8c8c1c550fb45b0a6ff21aaa517be036385541621961b5d873796",
            16,
        )
        .unwrap();
        tx_args.s = BigInt::from_str_radix(
            "55e8447d58d64ebc3038d9882886bbc3b0228c7ac77c71f4e811b97ed3f14b5a",
            16,
        )
        .unwrap();
        let tx = EthTx::Homestead(Box::new(tx_args.clone()));
        let expected_hash = ethereum_types::H256::from_str(
            "0x3ebc897150feeff6caa1b2e5992e347e8409e9e35fa30f7f5f8fcda3f7c965c7",
        )
        .unwrap();
        assert_eq!(expected_hash, tx.eth_hash().unwrap());
        // Note: `v` value 27 is for homestead
        tx_args.v = BigInt::from_str_radix("1b", 16).unwrap();
        let tx = EthTx::Homestead(Box::new(tx_args.clone()));
        let sig = tx.signature(calibnet::ETH_CHAIN_ID);
        assert!(sig.is_ok());
        assert!(tx
            .to_verifiable_signature(sig.unwrap().bytes().to_vec(), calibnet::ETH_CHAIN_ID)
            .is_ok());
        assert!(tx.rlp_unsigned_message(calibnet::ETH_CHAIN_ID).is_ok());
        assert!(tx.get_signed_message(calibnet::ETH_CHAIN_ID).is_ok());
    }

    #[quickcheck]
    fn u64_roundtrip(i: u64) {
        let bm = format_u64(i);
        if i == 0 {
            assert!(bm.is_empty());
        } else {
            // check that buffer doesn't start with zero
            let freezed = bm.freeze();
            assert!(!freezed.starts_with(&[0]));

            // roundtrip
            let mut padded = [0u8; 8];
            let bytes: &[u8] = &freezed.slice(..);
            padded[8 - bytes.len()..].copy_from_slice(bytes);
            assert_eq!(i, u64::from_be_bytes(padded));
        }
    }

    #[quickcheck]
    fn bigint_roundtrip(bi: num_bigint::BigInt) {
        match format_bigint(&bi) {
            Ok(bm) => {
                if bi.is_zero() {
                    assert!(bm.is_empty());
                } else {
                    // check that buffer doesn't start with zero
                    let freezed = bm.freeze();
                    assert!(!freezed.starts_with(&[0]));

                    // roundtrip
                    let unsigned = num_bigint::BigUint::from_be_bytes(&freezed.slice(..));
                    assert_eq!(bi, unsigned.into());
                }
            }
            Err(_) => {
                // fails in case of negative number
                assert!(bi.is_negative());
            }
        }
    }

    #[test]
    fn test_pad_leading_zeros() {
        // Case 1: Data is shorter than the target length
        let data = vec![1, 2, 3];
        let padded = pad_leading_zeros(data, 5);
        assert_eq!(padded, vec![0, 0, 1, 2, 3]);

        // Case 2: Data is already the target length
        let data = vec![4, 5, 6];
        let padded = pad_leading_zeros(data, 3);
        assert_eq!(padded, vec![4, 5, 6]);

        // Case 3: Data is longer than the target length (no padding should happen)
        let data = vec![7, 8, 9, 10];
        let padded = pad_leading_zeros(data, 3); // length is smaller
        assert_eq!(padded, vec![7, 8, 9, 10]); // Should return unchanged

        // Case 4: Data is empty, and the target length is greater than zero
        let data = vec![];
        let padded = pad_leading_zeros(data, 4);
        assert_eq!(padded, vec![0, 0, 0, 0]);
    }

    #[test]
    fn test_parse_eth_transaction() {
        // Legacy transaction
        let raw_tx = hex::decode(
            "f8cc04830406968419ca81cc94d0fb381fc644cdd5d694d35e1afb445527b9244b80b864d5b3d76d00000000000000000000000000000000000000000000000045466fa6fdcb80000000000000000000000000000000000000000000000000000000002e90edd000000000000000000000000000000000000000000000000000000000000001518083099681a0580b1d36c5a8c8c1c550fb45b0a6ff21aaa517be036385541621961b5d873796a055e8447d58d64ebc3038d9882886bbc3b0228c7ac77c71f4e811b97ed3f14b5a",
        )
        .expect("Invalid hex");
        let eth_tx = parse_eth_transaction(&raw_tx);
        assert!(eth_tx.is_ok());

        // EIP-1559 transaction
        let raw_tx = hex::decode(
            "02f901368304cb2f8201e68459682f008459682f7884023b53a794eb4a9cdb9f42d3a503d580a39b6e3736eb21fffd80b8c4383487be000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000660d4d120000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000003b6261666b726569656f6f75326d36356276376561786e7767656d7562723675787269696867366474646e6c7a663469616f37686c6e6a6d647372750000000000c001a0b9f0afb3fa8821fa414bac6056e613c61a8263ca341b59539096dbbc8600f530a0114a6a032347e132f115accc7664ccc61549be28f5b844c3fc170006feb72f24",
        )
        .expect("Invalid hex");
        let eth_tx = parse_eth_transaction(&raw_tx);
        assert!(eth_tx.is_ok());
    }

    #[test]
    fn test_derive_sender() {
        // Legacy transaction
        let raw_tx = hex::decode(
            "f8cc04830406968419ca81cc94d0fb381fc644cdd5d694d35e1afb445527b9244b80b864d5b3d76d00000000000000000000000000000000000000000000000045466fa6fdcb80000000000000000000000000000000000000000000000000000000002e90edd000000000000000000000000000000000000000000000000000000000000001518083099681a0580b1d36c5a8c8c1c550fb45b0a6ff21aaa517be036385541621961b5d873796a055e8447d58d64ebc3038d9882886bbc3b0228c7ac77c71f4e811b97ed3f14b5a",
        )
        .expect("Invalid hex");
        let eth_tx = parse_eth_transaction(&raw_tx).unwrap();
        let from = eth_tx.sender(calibnet::ETH_CHAIN_ID).unwrap();
        assert_eq!(
            EthAddress::from_filecoin_address(&from).unwrap(),
            EthAddress::from_str("0xEb1D0C87B7e33D0Ab44a397b675F0897295491C2").unwrap()
        );

        // EIP-1559 transaction
        let raw_tx = hex::decode(
            "02f901368304cb2f8201e68459682f008459682f7884023b53a794eb4a9cdb9f42d3a503d580a39b6e3736eb21fffd80b8c4383487be000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000660d4d120000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000003b6261666b726569656f6f75326d36356276376561786e7767656d7562723675787269696867366474646e6c7a663469616f37686c6e6a6d647372750000000000c001a0b9f0afb3fa8821fa414bac6056e613c61a8263ca341b59539096dbbc8600f530a0114a6a032347e132f115accc7664ccc61549be28f5b844c3fc170006feb72f24",
        )
        .expect("Invalid hex");
        let eth_tx = parse_eth_transaction(&raw_tx).unwrap();
        let from = eth_tx.sender(calibnet::ETH_CHAIN_ID).unwrap();
        assert_eq!(
            EthAddress::from_filecoin_address(&from).unwrap(),
            EthAddress::from_str("0x4fda4174D5D07C906395bfB77806287cc65Fd129").unwrap()
        );
    }

    #[test]
    fn test_parse_legacy_tx() {
        // Raw transaction hex for a Legacy transaction
        let raw_tx = hex::decode(
            "f8cc04830406968419ca81cc94d0fb381fc644cdd5d694d35e1afb445527b9244b80b864d5b3d76d00000000000000000000000000000000000000000000000045466fa6fdcb80000000000000000000000000000000000000000000000000000000002e90edd000000000000000000000000000000000000000000000000000000000000001518083099681a0580b1d36c5a8c8c1c550fb45b0a6ff21aaa517be036385541621961b5d873796a055e8447d58d64ebc3038d9882886bbc3b0228c7ac77c71f4e811b97ed3f14b5a",
        )
        .expect("Invalid hex");

        let result = parse_legacy_tx(&raw_tx);
        assert!(result.is_ok());
        let txn = result.unwrap();

        if let EthTx::Eip155(tx) = txn {
            assert_eq!(tx.chain_id, 314159);
            assert_eq!(tx.nonce, 4);
            assert_eq!(tx.gas_price, BigInt::from(263830u64));
            assert_eq!(tx.gas_limit, 432701900);
            assert_eq!(
                tx.to.unwrap(),
                EthAddress::from_str("0xd0fb381fc644cdd5d694d35e1afb445527b9244b").unwrap()
            );
            assert_eq!(tx.value, BigInt::from(0));
            assert_eq!(
                tx.v,
                BigInt::from_str_radix("099681", 16).expect("Invalid hex string")
            );
            assert_eq!(
                tx.r,
                BigInt::from_str_radix(
                    "580b1d36c5a8c8c1c550fb45b0a6ff21aaa517be036385541621961b5d873796",
                    16
                )
                .expect("Invalid hex string")
            );
            assert_eq!(
                tx.s,
                BigInt::from_str_radix(
                    "55e8447d58d64ebc3038d9882886bbc3b0228c7ac77c71f4e811b97ed3f14b5a",
                    16
                )
                .expect("Invalid hex string")
            );
        } else {
            panic!("Expected EIP-1559 transaction");
        }
    }

    #[test]
    fn test_parse_eip1559_tx() {
        // Raw transaction hex for the EIP-1559 transaction
        let raw_tx = hex::decode(
            "02f901368304cb2f8201e68459682f008459682f7884023b53a794eb4a9cdb9f42d3a503d580a39b6e3736eb21fffd80b8c4383487be000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000660d4d120000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000003b6261666b726569656f6f75326d36356276376561786e7767656d7562723675787269696867366474646e6c7a663469616f37686c6e6a6d647372750000000000c001a0b9f0afb3fa8821fa414bac6056e613c61a8263ca341b59539096dbbc8600f530a0114a6a032347e132f115accc7664ccc61549be28f5b844c3fc170006feb72f24"
        ).expect("Invalid hex");

        let result = parse_eip1559_tx(&raw_tx);
        assert!(result.is_ok());

        let txn = result.unwrap();

        if let EthTx::Eip1559(tx) = txn {
            assert_eq!(tx.chain_id, 314159);
            assert_eq!(tx.nonce, 486);
            assert_eq!(tx.max_fee_per_gas, BigInt::from(1500000120u64));
            assert_eq!(tx.max_priority_fee_per_gas, BigInt::from(1500000000u64));
            assert_eq!(tx.gas_limit, 37442471);
            assert_eq!(
                tx.to.unwrap(),
                EthAddress::from_str("0xeb4a9cdb9f42d3a503d580a39b6e3736eb21fffd").unwrap()
            );
            assert_eq!(tx.value, BigInt::from(0));
            assert_eq!(
                tx.v,
                BigInt::from_str_radix("01", 16).expect("Invalid hex string")
            );
            assert_eq!(
                tx.r,
                BigInt::from_str_radix(
                    "b9f0afb3fa8821fa414bac6056e613c61a8263ca341b59539096dbbc8600f530",
                    16
                )
                .expect("Invalid hex string")
            );
            assert_eq!(
                tx.s,
                BigInt::from_str_radix(
                    "114a6a032347e132f115accc7664ccc61549be28f5b844c3fc170006feb72f24",
                    16
                )
                .expect("Invalid hex string")
            );
        } else {
            panic!("Expected EIP-1559 transaction");
        }
    }
}

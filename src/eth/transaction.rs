// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{bail, ensure};
use cbor4ii::core::{dec::Decode as _, utils::SliceReader, Value};

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
    EthChainId,
};
// As per `ref-fvm`, which hardcodes it as well.
#[repr(u64)]
enum EAMMethod {
    CreateExternal = 4,
}

#[repr(u64)]
enum EVMMethod {
    // As per `ref-fvm`:
    // it is very unfortunate but the hasher creates a circular dependency, so we use the raw
    // number.
    // InvokeContract = frc42_dispatch::method_hash!("InvokeEVM"),
    InvokeContract = 3844450837,
}

/// Ethereum transaction which can be of different types.
/// The currently supported types are defined in [FIP-0091](https://github.com/filecoin-project/FIPs/blob/020bcb412ee20a2879b4a710337959c51b938d3b/FIPS/fip-0091.md).
#[allow(dead_code)]
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
        let (params, to) = get_eth_params_and_recipient(msg.message())?;

        // now we need to determine the transaction type based on the signature length
        let sig_len = msg.signature().bytes().len();

        // valid signature lengths are based on the chain ID, so we need to calculate it. This
        // shouldn't be a resource-intensive operation, but if it becomes one, we can do some
        // memoization.
        let valid_eip_155_signature_lengths = calc_valid_eip155_sig_len(eth_chain_id);

        let tx: Self = if sig_len == EIP_1559_SIG_LEN {
            let args = EthEip1559TxArgsBuilder::default()
                .chain_id(eth_chain_id)
                .nonce(msg.message().sequence())
                .to(to)
                .value(msg.value())
                .max_fee_per_gas(msg.message().gas_fee_cap())
                .max_priority_fee_per_gas(msg.message().gas_premium())
                .gas_limit(msg.message().gas_limit())
                .input(params)
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
                        .nonce(msg.message().sequence())
                        .to(to)
                        .value(msg.value())
                        .input(params)
                        .gas_price(msg.message().gas_fee_cap())
                        .gas_limit(msg.message().gas_limit())
                        .build()?
                        .with_signature(msg.signature())?;
                    EthTx::Homestead(Box::new(args))
                }
                EIP_155_SIG_PREFIX => {
                    let args = EthLegacyEip155TxArgsBuilder::default()
                        .nonce(msg.message().sequence())
                        .to(to)
                        .value(msg.value())
                        .input(params)
                        .gas_price(msg.message().gas_fee_cap())
                        .gas_limit(msg.message().gas_limit())
                        .build()?
                        .with_signature(msg.signature(), eth_chain_id)?;
                    EthTx::Eip155(Box::new(args))
                }
                _ => bail!("unsupported signature prefix"),
            }
        } else {
            bail!("unsupported signature length: {sig_len}");
        };

        Ok(tx)
    }

    pub(crate) fn is_eip1559(&self) -> bool {
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
fn get_eth_params_and_recipient(msg: &Message) -> anyhow::Result<(Vec<u8>, Option<EthAddress>)> {
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

#[cfg(test)]
pub(crate) mod tests {
    use std::str::FromStr as _;

    use num_bigint::ToBigUint as _;

    use crate::{
        networks::mainnet,
        shim::{crypto::Signature, econ::TokenAmount},
    };

    use super::*;
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
}

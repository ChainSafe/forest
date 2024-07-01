// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::bail;
use cbor4ii::core::{dec::Decode as _, utils::SliceReader, Value};

use crate::{
    message::{Message as _, SignedMessage},
    rpc::eth::{types::EthAddress},
    shim::{
        address::Address,
        crypto::{SignatureType},
        message::Message,
    },
};

use super::{
    eip_1559_transaction::{EthEip1559TxArgs, EIP_1559_SIG_LEN},
    eip_155_transaction::{calc_valid_eip155_sig_len, EthLegacyEip155TxArgs, EIP_155_SIG_PREFIX},
    homestead_transaction::{EthLegacyHomesteadTxArgs, HOMESTEAD_SIG_LEN, HOMESTEAD_SIG_PREFIX}, EthChainId,
};
// TODO: Is there a constant for this?
// As per `ref-fvm` which hardcodes it as well.
// InvokeContract = frc42_dispatch::method_hash!("InvokeEVM"),
#[repr(u64)]
enum EAMMethod {
    CreateExternal = 4,
}

#[repr(u64)]
enum EVMMethod {
    // it is very unfortunate but the hasher creates a circular dependency, so we use the raw
    // number.
    // InvokeContract = frc42_dispatch::method_hash!("InvokeEVM"),
    InvokeContract = 3844450837,
}

#[allow(dead_code)]
pub enum EthTx {
    Homestead(Box<EthLegacyHomesteadTxArgs>),
    Eip1559(Box<EthEip1559TxArgs>),
    Eip155(Box<EthLegacyEip155TxArgs>),
}

impl EthTx {
    pub fn from_signed_message(eth_chain_id: EthChainId, msg: &SignedMessage) -> anyhow::Result<Self> {
        if msg.signature().signature_type() != SignatureType::Delegated {
            bail!(
                "Signature is not delegated type, is {}",
                msg.signature().signature_type()
            );
        }
        if msg.message().version != 0 {
            bail!("Unsupported message version: {}", msg.message().version);
        }

        EthAddress::from_filecoin_address(&msg.from())?;

        let (params, to) = get_eth_params_and_recipient(msg.message())?;

        // now we need to determine the transaction type based on the signature length
        let sig_len = msg.signature().bytes().len();
        let valid_eip_155_signature_lengths = calc_valid_eip155_sig_len(eth_chain_id);

        let tx: Self = if sig_len == EIP_1559_SIG_LEN {
            let mut args = EthEip1559TxArgs {
                chain_id: eth_chain_id,
                nonce: msg.message().sequence(),
                to,
                value: msg.value().into(),
                max_fee_per_gas: msg.message().gas_fee_cap().into(),
                max_priority_fee_per_gas: msg.message().gas_premium().into(),
                gas_limit: msg.message().gas_limit(),
                ..Default::default()
            };
            args.initialise_signature(msg.signature())?;
            EthTx::Eip1559(Box::new(args))
        } else if sig_len == HOMESTEAD_SIG_LEN
            || sig_len == valid_eip_155_signature_lengths.0 as usize
            || sig_len == valid_eip_155_signature_lengths.1 as usize
        {
            // process based on the first byte of the signature
            // TODO refactor into smaller methods
            match *msg.signature().bytes().first().expect("infallible") {
                HOMESTEAD_SIG_PREFIX => {
                    let mut args = EthLegacyHomesteadTxArgs {
                        nonce: msg.message().sequence(),
                        to,
                        value: msg.value().into(),
                        input: params,
                        gas_price: msg.message().gas_fee_cap().into(),
                        gas_limit: msg.message().gas_limit(),
                        ..Default::default()
                    };
                    args.initialise_signature(msg.signature())?;
                    EthTx::Homestead(Box::new(args))
                }
                EIP_155_SIG_PREFIX => {
                    let mut args = EthLegacyEip155TxArgs {
                        nonce: msg.message().sequence(),
                        to,
                        value: msg.value().into(),
                        input: params,
                        gas_price: msg.message().gas_fee_cap().into(),
                        gas_limit: msg.message().gas_limit(),
                        ..Default::default()
                    };
                    args.initialise_signature(msg.signature(), eth_chain_id)?;
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
}

fn get_eth_params_and_recipient(msg: &Message) -> anyhow::Result<(Vec<u8>, Option<EthAddress>)> {
    let mut to = None;
    let mut params = vec![];

    if msg.version != 0 {
        bail!("unsupported msg version: {}", msg.version);
    }

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

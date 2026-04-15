// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::wallet::Key;
use crate::eth::{EthChainId, EthEip1559TxArgsBuilder, EthTx};
use crate::message::SignedMessage;
use crate::rpc::eth::types::EthAddress;
use crate::shim::{
    address::Address,
    crypto::{Signature, SignatureType},
    message::Message,
};
use crate::utils::encoding::{blake2b_256, keccak_256};
use bls_signatures::{PrivateKey as BlsPrivate, Serialize};

/// Return the uncompressed public key for a given private key and [`SignatureType`]
pub fn to_uncompressed_public_key(
    sig_type: SignatureType,
    private_key: &[u8],
) -> Result<Vec<u8>, Error> {
    match sig_type {
        SignatureType::Bls => Ok(BlsPrivate::from_bytes(private_key)?.public_key().as_bytes()),
        SignatureType::Secp256k1 | SignatureType::Delegated => {
            let private_key = k256::ecdsa::SigningKey::from_slice(private_key)?;
            let public_key = private_key.verifying_key();
            Ok(public_key.to_encoded_point(false).as_bytes().to_vec())
        }
    }
}

/// Return a new Address that is of a given [`SignatureType`] and uses the
/// supplied public key
pub fn new_address(sig_type: SignatureType, public_key: &[u8]) -> Result<Address, Error> {
    match sig_type {
        SignatureType::Bls => {
            let addr = Address::new_bls(public_key).map_err(|err| Error::Other(err.to_string()))?;
            Ok(addr)
        }
        SignatureType::Secp256k1 => {
            let addr =
                Address::new_secp256k1(public_key).map_err(|err| Error::Other(err.to_string()))?;
            Ok(addr)
        }
        SignatureType::Delegated => {
            let eth_addr = EthAddress::eth_address_from_uncompressed_public_key(public_key)?;
            let addr = eth_addr.to_filecoin_address()?;
            Ok(addr)
        }
    }
}

/// Sign takes in [`SignatureType`], private key and message. Returns a Signature
/// for that message
pub fn sign(sig_type: SignatureType, private_key: &[u8], msg: &[u8]) -> Result<Signature, Error> {
    let sign_k256 = |msg_hash, sig_type| -> Result<Signature, Error> {
        let priv_key = k256::ecdsa::SigningKey::from_slice(private_key)?;
        let (sig, recovery_id) = priv_key.sign_prehash_recoverable(msg_hash)?;
        let mut new_bytes = [0; 65];
        new_bytes[..64].copy_from_slice(&sig.to_bytes());
        new_bytes[64] = recovery_id.to_byte();
        Ok(Signature {
            sig_type,
            bytes: new_bytes.to_vec(),
        })
    };

    match sig_type {
        SignatureType::Bls => {
            let priv_key = BlsPrivate::from_bytes(private_key)?;
            // this returns a signature from bls-signatures, so we need to convert this to a
            // crypto signature
            let sig = priv_key.sign(msg);
            let crypto_sig = Signature::new_bls(sig.as_bytes());
            Ok(crypto_sig)
        }
        SignatureType::Secp256k1 => sign_k256(&blake2b_256(msg), sig_type),
        SignatureType::Delegated => sign_k256(&keccak_256(msg), sig_type),
    }
}

/// Generate a new private key
pub fn generate(sig_type: SignatureType) -> Result<Vec<u8>, Error> {
    let rng = &mut crate::utils::rand::forest_os_rng();
    match sig_type {
        SignatureType::Bls => {
            let key = BlsPrivate::generate(rng);
            Ok(key.as_bytes())
        }
        SignatureType::Secp256k1 | SignatureType::Delegated => {
            let key = k256::ecdsa::SigningKey::random(rng);
            Ok(key.to_bytes().to_vec())
        }
    }
}

/// Sign a Filecoin message using the appropriate signing bytes based on key type.
/// For delegated (EVM) keys, signs the RLP-encoded unsigned EIP-1559 transaction.
/// For other key types, signs the message CID bytes.
pub fn sign_message(
    key: &Key,
    message: &Message,
    eth_chain_id: EthChainId,
) -> anyhow::Result<SignedMessage> {
    let sig_type = *key.key_info.key_type();
    if sig_type == SignatureType::Delegated {
        let eth_tx_args = EthEip1559TxArgsBuilder::default()
            .chain_id(eth_chain_id)
            .unsigned_message(message)?
            .build()?;
        let eth_tx = EthTx::from(eth_tx_args);
        let sig = sign(
            sig_type,
            key.key_info.private_key(),
            &eth_tx.rlp_unsigned_message(eth_chain_id)?,
        )?;
        let unsigned_msg = eth_tx.get_unsigned_message(message.from, eth_chain_id)?;
        Ok(SignedMessage::new_unchecked(unsigned_msg, sig))
    } else {
        let sig = sign(
            sig_type,
            key.key_info.private_key(),
            message.cid().to_bytes().as_slice(),
        )?;
        Ok(SignedMessage::new_from_parts(message.clone(), sig)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eth::EVMMethod;
    use crate::key_management::generate_key;
    use crate::networks::calibnet;
    use crate::rpc::eth::types::EthAddress as RpcEthAddress;
    use crate::shim::econ::TokenAmount;

    const TEST_CHAIN_ID: EthChainId = calibnet::ETH_CHAIN_ID;

    fn make_secp_message(from: Address) -> Message {
        Message {
            from,
            to: Address::new_id(1),
            value: TokenAmount::from_whole(1),
            gas_limit: 10_000_000,
            gas_fee_cap: TokenAmount::from_nano(1500),
            gas_premium: TokenAmount::from_nano(1500),
            ..Message::default()
        }
    }

    fn make_delegated_message(from: Address) -> Message {
        let to_eth = RpcEthAddress::from(ethereum_types::H160::from_low_u64_be(42));
        let to = to_eth.to_filecoin_address().unwrap();
        Message {
            from,
            to,
            value: TokenAmount::from_whole(1),
            method_num: EVMMethod::InvokeContract as u64,
            gas_limit: 10_000_000,
            gas_fee_cap: TokenAmount::from_nano(1500),
            gas_premium: TokenAmount::from_nano(1500),
            ..Message::default()
        }
    }

    #[test]
    fn sign_message_secp256k1_uses_cid_bytes() {
        let key = generate_key(SignatureType::Secp256k1).unwrap();
        let msg = make_secp_message(key.address);

        let smsg = sign_message(&key, &msg, TEST_CHAIN_ID).unwrap();

        assert_eq!(smsg.message(), &msg);
        smsg.signature()
            .verify(&msg.cid().to_bytes(), &key.address)
            .expect("secp256k1 signature should verify against CID bytes");
    }

    #[test]
    fn sign_message_delegated_uses_rlp_bytes() {
        let key = generate_key(SignatureType::Delegated).unwrap();
        let msg = make_delegated_message(key.address);

        let smsg = sign_message(&key, &msg, TEST_CHAIN_ID).unwrap();

        let eth_tx_args = EthEip1559TxArgsBuilder::default()
            .chain_id(TEST_CHAIN_ID)
            .unsigned_message(&msg)
            .unwrap()
            .build()
            .unwrap();
        let eth_tx = EthTx::from(eth_tx_args);
        let expected_rlp = eth_tx.rlp_unsigned_message(TEST_CHAIN_ID).unwrap();

        smsg.signature()
            .verify(&expected_rlp, &key.address)
            .expect("delegated signature should verify against RLP bytes");
    }

    #[test]
    fn sign_message_delegated_passes_authenticate_msg() {
        let key = generate_key(SignatureType::Delegated).unwrap();
        let msg = make_delegated_message(key.address);

        let smsg = sign_message(&key, &msg, TEST_CHAIN_ID).unwrap();

        smsg.signature()
            .authenticate_msg(TEST_CHAIN_ID, &smsg, &key.address)
            .expect("delegated signed message should pass authenticate_msg");
    }

    #[test]
    fn sign_message_secp256k1_passes_new_from_parts() {
        let key = generate_key(SignatureType::Secp256k1).unwrap();
        let msg = make_secp_message(key.address);

        let smsg = sign_message(&key, &msg, TEST_CHAIN_ID).unwrap();

        SignedMessage::new_from_parts(smsg.message().clone(), smsg.signature().clone())
            .expect("secp256k1 signed message should pass new_from_parts verification");
    }

    #[test]
    fn sign_message_delegated_reconstructs_message_via_roundtrip() {
        let key = generate_key(SignatureType::Delegated).unwrap();
        let msg = make_delegated_message(key.address);

        let smsg = sign_message(&key, &msg, TEST_CHAIN_ID).unwrap();

        let eth_tx_args = EthEip1559TxArgsBuilder::default()
            .chain_id(TEST_CHAIN_ID)
            .unsigned_message(&msg)
            .unwrap()
            .build()
            .unwrap();
        let eth_tx = EthTx::from(eth_tx_args);
        let expected_msg = eth_tx
            .get_unsigned_message(key.address, TEST_CHAIN_ID)
            .unwrap();

        assert_eq!(
            smsg.message().cid(),
            expected_msg.cid(),
            "delegated path should use the EthTx-roundtripped message"
        );
    }
}

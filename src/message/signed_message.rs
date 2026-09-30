// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{MessageRead, MessageReadWrite};
use crate::eth::EthChainId;
use crate::shim::{
    address::Address,
    crypto::{Signature, SignatureType},
    econ::TokenAmount,
    message::Message,
};
use crate::utils::cid::{EncodedCbor, Memo};
use get_size2::GetSize;

/// Represents a wrapped message with signature bytes.
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[derive(Clone, Debug, PartialEq, Eq, Hash, GetSize)]
pub struct SignedMessage {
    message: Message,
    signature: Signature,
    /// The memoized CID and length of the signed encoding. Cleared whenever the message or
    /// signature is replaced, so it cannot outlive what it was derived from, and excluded from
    /// the derived `PartialEq`/`Eq`/`Hash`/`Debug` by [`Memo`].
    #[cfg_attr(test, arbitrary(gen(|_| Memo::default())))]
    encoded: Memo,
}

impl serde::Serialize for SignedMessage {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (&self.message, &self.signature).serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for SignedMessage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (message, signature) = serde::Deserialize::deserialize(deserializer)?;
        Ok(Self {
            message,
            signature,
            encoded: Memo::default(),
        })
    }
}

impl SignedMessage {
    /// Generate a new signed message from fields.
    /// The signature will be verified.
    pub fn new_from_parts(message: Message, signature: Signature) -> anyhow::Result<SignedMessage> {
        signature.verify(&message.cid().to_bytes(), &message.from())?;
        Ok(SignedMessage::new_unchecked(message, signature))
    }

    /// Generate a new signed message from fields.
    /// The signature will not be verified.
    pub fn new_unchecked(message: Message, signature: Signature) -> SignedMessage {
        SignedMessage {
            message,
            signature,
            encoded: Memo::default(),
        }
    }

    /// Returns reference to the unsigned message.
    pub fn message(&self) -> &Message {
        &self.message
    }

    /// Returns signature of the signed message.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn set_signature(&mut self, signature: Signature) {
        self.encoded.clear();
        self.signature = signature;
    }

    /// Drops the memo up front, since the caller may change anything the signed encoding is derived from.
    pub fn message_mut(&mut self) -> &mut Message {
        self.encoded.clear();
        &mut self.message
    }

    /// Consumes self and returns it's unsigned message.
    pub fn into_message(self) -> Message {
        self.message
    }

    pub fn into_parts(self) -> (Message, Signature) {
        (self.message, self.signature)
    }

    /// Checks if the signed message is a BLS message.
    pub fn is_bls(&self) -> bool {
        self.signature.signature_type() == SignatureType::Bls
    }

    /// Checks if the signed message is a SECP message.
    pub fn is_secp256k1(&self) -> bool {
        self.signature.signature_type() == SignatureType::Secp256k1
    }

    /// Checks if the signed message is a delegated message.
    pub fn is_delegated(&self) -> bool {
        self.signature.signature_type() == SignatureType::Delegated
    }

    /// Verifies that the from address of the message generated the signature.
    pub fn verify(&self, eth_chain_id: EthChainId) -> anyhow::Result<()> {
        self.signature
            .authenticate_msg(eth_chain_id, self, &self.from())
    }

    // Important note: `msg.cid()` is different from
    // `Cid::from_cbor_blake2b256(msg)`. The behavior comes from Lotus, and
    // Lotus, by, definition, is correct.
    pub fn cid(&self) -> cid::Cid {
        if self.is_bls() {
            self.message.cid()
        } else {
            self.encoded().cid()
        }
    }

    /// The CID of the signed encoding, which is what a block's `SECP` message root is built from.
    /// Differs from [`SignedMessage::cid`] only for a BLS signature, which a `SECP` message list
    /// may not carry.
    pub fn signed_cid(&self) -> cid::Cid {
        self.encoded().cid()
    }

    /// The length of the signed encoding, which is what a message's size on the wire is measured
    /// against. Differs from [`MessageRead::chain_length`] for a BLS signature, which is excluded
    /// from the chain length but not from the message itself.
    pub fn signed_encoded_len(&self) -> usize {
        self.encoded().byte_len()
    }

    fn encoded(&self) -> &EncodedCbor {
        self.encoded.get_or_init(|| {
            EncodedCbor::compute(self).expect("message serialization is infallible")
        })
    }

    /// Creates a mock signed message for testing purposes. The signature check will fail if
    /// invoked.
    #[cfg(test)]
    pub fn mock_bls_signed_message(message: Message) -> SignedMessage {
        let signature = Signature::new_bls(vec![0; crate::shim::crypto::BLS_SIG_LEN]);
        SignedMessage::new_unchecked(message, signature)
    }
}

impl MessageRead for SignedMessage {
    fn vm_message(&self) -> &Message {
        &self.message
    }
    fn chain_length(&self) -> anyhow::Result<usize> {
        Ok(match self.signature.signature_type() {
            // BLS chain message length doesn't include the signature
            SignatureType::Bls => self.message.encoded_len(),
            // SECP and Delegated chain message length includes the signature
            SignatureType::Secp256k1 | SignatureType::Delegated => self.encoded().byte_len(),
        })
    }
    fn from(&self) -> Address {
        self.message.from()
    }
    fn to(&self) -> Address {
        self.message.to()
    }
    fn sequence(&self) -> u64 {
        self.message.sequence()
    }
    fn value(&self) -> &TokenAmount {
        self.message.value()
    }
    fn gas_limit(&self) -> u64 {
        self.message.gas_limit()
    }
    fn required_funds(&self) -> TokenAmount {
        self.message.required_funds()
    }
    fn gas_fee_cap(&self) -> &TokenAmount {
        self.message.gas_fee_cap()
    }
    fn gas_premium(&self) -> &TokenAmount {
        self.message.gas_premium()
    }
}

impl MessageReadWrite for SignedMessage {
    fn set_gas_limit(&mut self, token_amount: u64) {
        self.message_mut().set_gas_limit(token_amount);
    }
    fn set_sequence(&mut self, new_sequence: u64) {
        self.message_mut().set_sequence(new_sequence);
    }
    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        self.message_mut().set_gas_fee_cap(cap)
    }
    fn set_gas_premium(&mut self, prem: TokenAmount) {
        self.message_mut().set_gas_premium(prem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shim::{
        address::Address,
        crypto::{BLS_SIG_LEN, SECP_SIG_LEN, Signature},
        message::Message,
    };
    use crate::utils::cid::CidCborExt as _;
    use cid::Cid;
    use fvm_ipld_encoding::to_vec;
    use quickcheck_macros::quickcheck;

    fn hash_of<T: std::hash::Hash>(value: &T) -> u64 {
        use std::hash::Hasher as _;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// `Arbitrary` always yields cold memos, so both must be warmed here to reach the case the
    /// derived `PartialEq`/`Hash`/`Debug` must stay blind to.
    #[quickcheck]
    fn computing_the_memos_is_invisible(msg: SignedMessage) -> bool {
        let warm = msg.clone();
        let (_, _) = (warm.cid(), warm.signed_cid());
        warm == msg && hash_of(&warm) == hash_of(&msg) && format!("{warm:?}") == format!("{msg:?}")
    }

    #[quickcheck]
    fn signed_cid_and_len_match_the_unmemoized_path(msg: SignedMessage) -> bool {
        msg.signed_cid() == Cid::from_cbor_blake2b256(&msg).unwrap()
            && msg.signed_encoded_len() == to_vec(&msg).unwrap().len()
    }

    #[track_caller]
    fn assert_measures_and_hashes(signed: &SignedMessage, encoded: &[u8]) {
        assert_eq!(signed.chain_length().unwrap(), encoded.len());
        assert_eq!(
            signed.cid(),
            Cid::from_cbor_encoded_raw_bytes_blake2b256(encoded)
        );
    }

    /// Anchors both values to the Lotus rule, which a test derived from the same `match` as the
    /// implementation cannot do.
    #[test]
    fn chain_length_and_cid_follow_signature_type() {
        let message = Message::builder()
            .to(Address::new_id(1))
            .from(Address::new_id(2))
            .build();

        // BLS signatures are aggregated into the block header, so they count for neither value.
        let bls =
            SignedMessage::new_unchecked(message.clone(), Signature::new_bls(vec![0; BLS_SIG_LEN]));
        assert_measures_and_hashes(&bls, &to_vec(&message).unwrap());

        for signature in [
            Signature::new_secp256k1(vec![0; SECP_SIG_LEN]),
            Signature::new_delegated(vec![0; SECP_SIG_LEN]),
        ] {
            let signed = SignedMessage::new_unchecked(message.clone(), signature);
            assert_measures_and_hashes(&signed, &to_vec(&signed).unwrap());
        }
    }

    /// Pins the hand-written [`serde`] code to the `Serialize_tuple` derive it replaced, which
    /// no round-trip test can do on its own.
    #[quickcheck]
    fn signed_encoding_matches_the_tuple_derive(msg: SignedMessage) -> bool {
        use fvm_ipld_encoding::tuple::*;

        #[derive(Serialize_tuple)]
        struct Reference<'a> {
            message: &'a Message,
            signature: &'a Signature,
        }

        to_vec(&msg).unwrap()
            == to_vec(&Reference {
                message: msg.message(),
                signature: msg.signature(),
            })
            .unwrap()
    }

    /// The signature type selects one encoding for both the CID and the chain length, so the two
    /// cannot be allowed to disagree about which bytes they mean.
    #[quickcheck]
    fn chain_length_measures_the_bytes_the_cid_hashes(msg: SignedMessage) -> bool {
        [to_vec(msg.message()).unwrap(), to_vec(&msg).unwrap()]
            .into_iter()
            .find(|bytes| Cid::from_cbor_encoded_raw_bytes_blake2b256(bytes) == msg.cid())
            .is_some_and(|bytes| msg.chain_length().unwrap() == bytes.len())
    }

    /// Both values are derived from an encoding of the message, so neither may be memoized without
    /// being invalidated when the message or the signature type changes.
    #[test]
    fn chain_length_and_cid_track_mutation() {
        let message = Message::builder()
            .to(Address::new_id(1))
            .from(Address::new_id(2))
            .build();
        let secp = || {
            SignedMessage::new_unchecked(
                message.clone(),
                Signature::new_secp256k1(vec![0; SECP_SIG_LEN]),
            )
        };

        let mut signed = secp();
        let (length, cid) = (signed.chain_length().unwrap(), signed.cid());
        signed.set_gas_limit(u64::from(u32::MAX));
        assert_ne!(signed.chain_length().unwrap(), length);
        assert_ne!(signed.cid(), cid);

        let mut signed = secp();
        let secp_length = signed.chain_length().unwrap();
        signed.set_signature(Signature::new_bls(vec![0; BLS_SIG_LEN]));
        assert_ne!(signed.chain_length().unwrap(), secp_length);
        assert_eq!(
            signed.chain_length().unwrap(),
            to_vec(&message).unwrap().len()
        );
        assert_eq!(signed.cid(), message.cid());
    }
}

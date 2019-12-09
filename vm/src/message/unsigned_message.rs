use super::Message;

use address::Address;
use derive_builder::Builder;
use encoding::{Cbor, CodecProtocol, Error as EncodingError};
use num_bigint::BigUint;

/// Default Unsigned VM message type which includes all data needed for a state transition
///
/// Usage:
/// ```
/// use vm::message::UnsignedMessage;
/// use num_bigint::BigUint;
/// use address::Address;
///
/// // Use the builder pattern to generate a message
/// let message = UnsignedMessage::builder()
///     .to(Address::new_id(0).unwrap())
///     .from(Address::new_id(1).unwrap())
///     .sequence(0) // optional
///     .value(BigUint::default()) // optional
///     .method_num(0) // optional
///     .params(vec![]) // optional
///     .gas_limit(BigUint::default()) // optional
///     .gas_price(BigUint::default()) // optional
///     .build()
///     .unwrap();
///
/// // Commands can be chained, or built seperately
/// let mut message_builder = UnsignedMessage::builder();
/// message_builder.sequence(1);
/// message_builder.from(Address::new_id(0).unwrap());
/// message_builder.to(Address::new_id(1).unwrap());
/// let _ = message_builder.build().unwrap();
/// ```
#[derive(PartialEq, Clone, Debug, Builder)]
#[builder(name = "MessageBuilder")]
pub struct UnsignedMessage {
    from: Address,
    to: Address,
    #[builder(default)]
    sequence: u64,
    #[builder(default)]
    value: BigUint,
    #[builder(default)]
    method_num: u64,
    #[builder(default)]
    params: Vec<u8>,
    #[builder(default)]
    gas_price: BigUint,
    #[builder(default)]
    gas_limit: BigUint,
}

impl UnsignedMessage {
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }
}

impl Message for UnsignedMessage {
    /// from returns the from address of the message
    fn from(&self) -> Address {
        self.from.clone()
    }
    /// to returns the destination address of the message
    fn to(&self) -> Address {
        self.to.clone()
    }
    /// sequence returns the message sequence or nonce
    fn sequence(&self) -> u64 {
        self.sequence
    }
    /// value returns the amount sent in message
    fn value(&self) -> BigUint {
        self.value.clone()
    }
    /// method_num returns the method number to be called
    fn method_num(&self) -> u64 {
        self.method_num
    }
    /// params returns the encoded parameters for the method call
    fn params(&self) -> Vec<u8> {
        self.params.clone()
    }
    /// gas_price returns gas price for the message
    fn gas_price(&self) -> BigUint {
        self.gas_price.clone()
    }
    /// gas_limit returns the gas limit for the message
    fn gas_limit(&self) -> BigUint {
        self.gas_limit.clone()
    }
}

impl Cbor for UnsignedMessage {
    fn unmarshal_cbor(_bz: &[u8]) -> Result<Self, EncodingError> {
        // TODO
        Err(EncodingError::Unmarshalling {
            description: "Not Implemented".to_string(),
            protocol: CodecProtocol::Cbor,
        })
    }
    fn marshal_cbor(&self) -> Result<Vec<u8>, EncodingError> {
        // TODO
        Err(EncodingError::Marshalling {
            description: format!("Not implemented, data: {:?}", self),
            protocol: CodecProtocol::Cbor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_builder() {
        let to_addr = Address::new_id(1).unwrap();
        let from_addr = Address::new_id(2).unwrap();

        // Able to build with chaining just to and from fields
        let message = UnsignedMessage::builder()
            .to(to_addr.clone())
            .from(from_addr.clone())
            .sequence(0)
            .value(BigUint::default())
            .method_num(0)
            .params(vec![])
            .gas_limit(BigUint::default())
            .gas_price(BigUint::default())
            .build()
            .unwrap();
        assert_eq!(
            message,
            UnsignedMessage {
                from: from_addr.clone(),
                to: to_addr.clone(),
                sequence: 0,
                value: BigUint::default(),
                method_num: 0,
                params: Vec::default(),
                gas_price: BigUint::default(),
                gas_limit: BigUint::default(),
            }
        );

        let mut mb = UnsignedMessage::builder();
        mb.to(to_addr.clone());
        mb.from(from_addr.clone());
        {
            // Test scoped modification still applies to builder
            mb.sequence(1);
        }
        // test unwrapping
        let u_msg = mb.build().unwrap();
        assert_eq!(
            u_msg,
            UnsignedMessage {
                from: from_addr.clone(),
                to: to_addr.clone(),
                sequence: 1,
                value: BigUint::default(),
                method_num: 0,
                params: Vec::default(),
                gas_price: BigUint::default(),
                gas_limit: BigUint::default(),
            }
        );
    }
}

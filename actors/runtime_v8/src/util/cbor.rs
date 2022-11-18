use fvm_ipld_encoding::{to_vec, RawBytes};
use serde::{de, ser};

use crate::ActorError;

/// Serializes a structure as a CBOR vector of bytes, returning a serialization error on failure.
/// `desc` is a noun phrase for the object being serialized, included in any error message.
pub fn serialize_vec<T>(value: &T, desc: &str) -> Result<Vec<u8>, ActorError>
where
    T: ser::Serialize + ?Sized,
{
    to_vec(value)
        .map_err(|e| ActorError::serialization(format!("failed to serialize {}: {}", desc, e)))
}

/// Serializes a structure as CBOR bytes, returning a serialization error on failure.
/// `desc` is a noun phrase for the object being serialized, included in any error message.
pub fn serialize<T>(value: &T, desc: &str) -> Result<RawBytes, ActorError>
where
    T: ser::Serialize + ?Sized,
{
    Ok(RawBytes::new(serialize_vec(value, desc)?))
}

/// Deserialises CBOR-encoded bytes as a structure, returning a serialization error on failure.
/// `desc` is a noun phrase for the object being deserialized, included in any error message.
pub fn deserialize<O: de::DeserializeOwned>(v: &RawBytes, desc: &str) -> Result<O, ActorError> {
    v.deserialize()
        .map_err(|e| ActorError::serialization(format!("failed to deserialize {}: {}", desc, e)))
}

/// Deserialises CBOR-encoded bytes as a method parameters object.
pub fn deserialize_params<O: de::DeserializeOwned>(params: &RawBytes) -> Result<O, ActorError> {
    deserialize(params, "method parameters")
}

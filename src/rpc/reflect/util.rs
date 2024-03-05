use std::fmt::Display;

use serde::{de::Visitor, forward_to_deserialize_any, Deserialize, Deserializer};

/// "Introspection" by tracing a [`Deserialize`] to see if it's [`Option`]-like.
///
/// This allows us to smoothly operate between rust functions that take an optional
/// paramater, and [`crate::openrpc_types::ContentDescriptor::required`].
pub trait Optional<'de>: Deserialize<'de> {
    fn optional() -> bool {
        #[derive(Default)]
        struct DummyDeserializer;

        #[derive(thiserror::Error, Debug)]
        #[error("")]
        struct DeserializeOptionWasCalled(bool);

        impl serde::de::Error for DeserializeOptionWasCalled {
            fn custom<T: Display>(_: T) -> Self {
                Self(false)
            }
        }

        impl<'de> Deserializer<'de> for DummyDeserializer {
            type Error = DeserializeOptionWasCalled;

            fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
                Err(DeserializeOptionWasCalled(false))
            }

            fn deserialize_option<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
                Err(DeserializeOptionWasCalled(true))
            }

            forward_to_deserialize_any! {
                bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
                bytes byte_buf unit unit_struct newtype_struct seq tuple
                tuple_struct map struct enum identifier ignored_any
            }
        }

        let Err(DeserializeOptionWasCalled(optional)) = Self::deserialize(DummyDeserializer) else {
            unreachable!("DummyDeserializer never returns Ok(..)")
        };
        optional
    }
    /// # Panics
    /// - This is only safe to call if [`Optional::optional`] returns `true`.
    fn unwrap_none() -> Self {
        Self::deserialize(serde_json::Value::Null)
            .expect("`null` json values should deserialize to a `None` for option-like types")
    }
}

impl<'de, T> Optional<'de> for T where T: Deserialize<'de> {}

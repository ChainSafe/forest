// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::de::{self, SeqAccess, Visitor};
use serde::Deserialize;
use std::fmt;
use std::marker::PhantomData;

/// Helper visitor to match Go's default behaviour of serializing uninitialized slices as null.
/// This will be able to deserialize null as empty Vectors of the type.
///
/// T indicates the return type, and D is an optional generic to override the
#[derive(Default)]
pub struct GoVecVisitor<T, D = T> {
    return_type: PhantomData<T>,
    deserialize_type: PhantomData<D>,
}

impl<T, D> GoVecVisitor<T, D> {
    pub fn new() -> Self {
        Self {
            return_type: PhantomData,
            deserialize_type: PhantomData,
        }
    }
}

impl<'de, T, D> Visitor<'de> for GoVecVisitor<T, D>
where
    T: From<D>,
    D: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a vector of serializable objects or null")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Vec<T>, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut vec = Vec::new();
        while let Some(elem) = seq.next_element::<D>()? {
            vec.push(T::from(elem));
        }
        Ok(vec)
    }
    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Vec::new())
    }
    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Deserializer};
    use serde_json::from_str;

    #[test]
    fn test_json_basic() {
        struct BasicJson(Vec<u8>);
        impl<'de> Deserialize<'de> for BasicJson {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Ok(Self(
                    deserializer.deserialize_any(GoVecVisitor::<u8>::new())?,
                ))
            }
        }

        let null_json = r#"null"#;
        let BasicJson(deserialized) = from_str(null_json).unwrap();
        assert_eq!(deserialized, [0u8; 0]);

        let empty_array = r#"[]"#;
        let BasicJson(deserialized) = from_str(empty_array).unwrap();
        assert_eq!(deserialized, [0u8; 0]);

        let with_values = r#"[1, 2]"#;
        let BasicJson(deserialized) = from_str(with_values).unwrap();
        assert_eq!(deserialized, [1, 2]);
    }

    #[test]
    fn serialize_through_other() {
        #[derive(Debug, PartialEq, Default)]
        struct TestOther(String);
        impl From<u8> for TestOther {
            fn from(i: u8) -> Self {
                Self(i.to_string())
            }
        }

        struct BasicJson(Vec<TestOther>);
        impl<'de> Deserialize<'de> for BasicJson {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Ok(Self(deserializer.deserialize_any(GoVecVisitor::<
                    TestOther,
                    u8,
                >::new(
                ))?))
            }
        }

        let null_json = r#"null"#;
        let BasicJson(deserialized) = from_str(null_json).unwrap();
        assert_eq!(deserialized, []);

        let empty_array = r#"[]"#;
        let BasicJson(deserialized) = from_str(empty_array).unwrap();
        assert_eq!(deserialized, []);

        let with_values = r#"[1, 2]"#;
        let BasicJson(deserialized) = from_str(with_values).unwrap();
        assert_eq!(
            deserialized,
            [TestOther("1".to_owned()), TestOther("2".to_owned())]
        );
    }
}

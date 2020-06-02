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
    T: Deserialize<'de> + From<D>,
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

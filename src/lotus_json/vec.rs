// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

pub struct VecLotusJson<T>(Vec<T>); // need a struct to handle the serialization of an empty vec as null

impl<T> HasLotusJson for Vec<T>
where
    T: HasLotusJson,
{
    type LotusJson = VecLotusJson<T::LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!("only Vec<Cid> is tested, below")
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        VecLotusJson(self.into_iter().map(T::into_lotus_json).collect())
    }

    fn from_lotus_json(VecLotusJson(vec): Self::LotusJson) -> Self {
        vec.into_iter().map(T::from_lotus_json).collect()
    }
}

#[test]
fn shapshots() {
    assert_one_snapshot(json!([{"/": "baeaaaaa"}]), vec![::cid::Cid::default()]);
}

#[cfg(test)]
quickcheck! {
    fn quickcheck(val: Vec<::cid::Cid>) -> () {
        assert_unchanged_via_json(val)
    }
}

impl<T> Serialize for VecLotusJson<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0.is_empty() {
            true => serializer.serialize_none(),
            false => self.0.serialize(serializer),
        }
    }
}

impl<'de, T> Deserialize<'de> for VecLotusJson<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<Vec<T>>::deserialize(deserializer)
            .map(Option::unwrap_or_default)
            .map(Self)
    }
}

impl<T> JsonSchema for VecLotusJson<T>
where
    T: JsonSchema,
{
    fn schema_name() -> String {
        std::any::type_name::<Self>().to_string()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        Option::<Vec<T>>::json_schema(gen)
    }
}

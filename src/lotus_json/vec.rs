use super::*;

pub struct VecLotusJson<T>(Vec<T>);

impl<T> HasLotusJson for Vec<T>
where
    T: HasLotusJson,
{
    type LotusJson = VecLotusJson<T::LotusJson>;
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

// VecLotusJson<T::LotusJson> -> Vec<T>
impl<T> From<VecLotusJson<T::LotusJson>> for Vec<T>
where
    T: HasLotusJson,
    T::LotusJson: Into<T>,
{
    fn from(value: VecLotusJson<T::LotusJson>) -> Self {
        value.0.into_iter().map(Into::into).collect()
    }
}

// Vec<T> -> VecLotusJson<T::LotusJson>
impl<T> From<Vec<T>> for VecLotusJson<T::LotusJson>
where
    T: HasLotusJson + Into<T::LotusJson>,
{
    fn from(value: Vec<T>) -> Self {
        Self(value.into_iter().map(Into::into).collect())
    }
}

#[test]
fn test() {
    assert_snapshot(json!([{"/": "baeaaaaa"}]), vec![::cid::Cid::default()]);
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: Vec<::cid::Cid>) -> bool {
        assert_via_json(val);
        true
    }
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

// TODO(forest): https://github.com/ChainSafe/forest/issues/4032
//               Remove this - users should use `Option<LotusJson<T>>` instead
//               of LotusJson<Option<T>>
impl<T> HasLotusJson for Option<T>
where
    T: HasLotusJson,
{
    type LotusJson = Option<T::LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!("only Option<Cid> is tested, below")
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.map(T::into_lotus_json)
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        lotus_json.map(T::from_lotus_json)
    }
}

#[test]
fn shapshots() {
    assert_one_snapshot(json!({"/": "baeaaaaa"}), Some(::cid::Cid::default()));
    assert_one_snapshot(json!(null), None::<::cid::Cid>);
}

#[cfg(test)]
#[quickcheck_macros::quickcheck]
fn quickcheck(val: Option<::cid::Cid>) {
    assert_unchanged_via_json(val)
}

/// Regression test for <https://github.com/ChainSafe/forest/issues/4331>.
///
/// `LotusJson<Option<T>>` must be rendered as an optional value that permits
/// `null` in the generated JSON/OpenRPC schema, so consumers know the field
/// may be absent. The non-optional `LotusJson<T>` counterpart must not permit
/// `null`.
#[test]
fn optional_is_nullable_in_schema() {
    let optional = schemars::schema_for!(LotusJson<Option<::cid::Cid>>);
    assert!(
        schema_allows_null(optional.as_value()),
        "LotusJson<Option<T>> schema must permit null: {optional:?}"
    );

    let required = schemars::schema_for!(LotusJson<::cid::Cid>);
    assert!(
        !schema_allows_null(required.as_value()),
        "LotusJson<T> schema must not permit null: {required:?}"
    );
}

/// Returns `true` if `schema` accepts a JSON `null`, whether expressed as a
/// `"type": "null"` / `"type": [.., "null"]` union or as a `null` branch of an
/// `anyOf`/`oneOf`.
#[cfg(test)]
fn schema_allows_null(schema: &serde_json::Value) -> bool {
    let type_allows_null = match schema.get("type") {
        Some(serde_json::Value::String(s)) => s == "null",
        Some(serde_json::Value::Array(types)) => types.iter().any(|t| t.as_str() == Some("null")),
        _ => false,
    };
    type_allows_null
        || ["anyOf", "oneOf"].iter().any(|key| {
            matches!(schema.get(key), Some(serde_json::Value::Array(variants))
                if variants.iter().any(schema_allows_null))
        })
}

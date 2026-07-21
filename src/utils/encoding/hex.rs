// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Hex encoding/decoding built on the SIMD-accelerated `faster-hex` crate, a drop-in
//! replacement for the `hex` crate: import this module and `hex::encode`/`hex::decode`
//! call sites keep working. See benchmark results in
//! <https://github.com/ChainSafe/forest/pull/7395>.

#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error(transparent)]
pub struct DecodeError(#[from] faster_hex_private::Error);

/// Lower-case hex encoding, without prefix.
pub fn encode(data: impl AsRef<[u8]>) -> String {
    faster_hex_private::hex_string(data.as_ref())
}

/// Lower-case hex encoding with a `0x` prefix.
pub fn encode_prefixed(data: impl AsRef<[u8]>) -> String {
    let data = data.as_ref();
    let mut buf = vec![0; 2 + data.len() * 2];
    let (prefix, digits) = buf.split_at_mut(2);
    prefix.copy_from_slice(b"0x");
    faster_hex_private::hex_encode(data, digits).expect("output buffer is sized to fit");
    debug_assert!(buf.is_ascii());
    // SAFETY: the prefix and the `hex_encode` output are ASCII.
    unsafe { String::from_utf8_unchecked(buf) }
}

/// Decodes hex digits (upper, lower or mixed case, no `0x` prefix) into bytes.
pub fn decode(input: impl AsRef<[u8]>) -> Result<Vec<u8>, DecodeError> {
    let input = input.as_ref();
    let mut out = vec![0; input.len() / 2];
    faster_hex_private::hex_decode(input, &mut out)?;
    Ok(out)
}

/// Usage: `#[serde(with = "crate::utils::encoding::hex::serde")]`, a drop-in
/// replacement for `hex::serde`: lower-case, no prefix.
pub mod serde {
    use serde::{Deserialize as _, Deserializer, Serializer, de};

    pub fn serialize<S: Serializer>(
        data: impl AsRef<[u8]>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::encode(data))
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: TryFrom<Vec<u8>>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = super::decode(&s).map_err(de::Error::custom)?;
        let len = bytes.len();
        T::try_from(bytes).map_err(|_| de::Error::custom(format!("invalid length {len}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn encode_matches_hex_crate(data: Vec<u8>) -> bool {
        encode(&data) == ::hex::encode(&data)
    }

    #[quickcheck]
    fn encode_prefixed_is_ascii_with_prefix(data: Vec<u8>) -> bool {
        let s = encode_prefixed(&data);
        s.is_ascii() && s.strip_prefix("0x") == Some(encode(&data).as_str())
    }

    /// Accept/reject boundary and decoded bytes agree with the `hex` crate this
    /// module replaced, on both guaranteed-valid and arbitrary input.
    #[quickcheck]
    fn decode_matches_hex_crate(data: Vec<u8>, junk: String) -> bool {
        [::hex::encode(&data), junk]
            .iter()
            .all(|s| match (decode(s), ::hex::decode(s)) {
                (Ok(ours), Ok(theirs)) => ours == theirs,
                (Err(_), Err(_)) => true,
                _ => false,
            })
    }

    #[quickcheck]
    fn decode_roundtrip_any_case(data: Vec<u8>, flips: Vec<bool>) -> bool {
        let s: String = encode(&data)
            .chars()
            .zip(flips.into_iter().chain(std::iter::repeat(false)))
            .map(|(c, up)| if up { c.to_ascii_uppercase() } else { c })
            .collect();
        decode(&s).unwrap() == data
    }

    #[quickcheck]
    fn decode_no_panic(input: Vec<u8>) {
        let _ = decode(&input);
    }

    #[quickcheck]
    fn serde_matches_hex_crate(data: Vec<u8>) -> bool {
        #[derive(::serde::Serialize, ::serde::Deserialize)]
        struct Ours(#[serde(with = "crate::utils::encoding::hex::serde")] Vec<u8>);
        #[derive(::serde::Serialize)]
        struct Theirs(#[serde(with = "::hex::serde")] Vec<u8>);

        let ours = serde_json::to_string(&Ours(data.clone())).unwrap();
        ours == serde_json::to_string(&Theirs(data.clone())).unwrap()
            && serde_json::from_str::<Ours>(&ours).unwrap().0 == data
    }

    #[test]
    fn encode_matches_expectations() {
        assert_eq!(encode([]), "");
        assert_eq!(encode([0x00, 0xab, 0xff]), "00abff");
        assert_eq!(encode_prefixed([]), "0x");
        assert_eq!(encode_prefixed([0x00, 0xab, 0xff]), "0x00abff");
    }

    #[test]
    fn decode_accepts_mixed_case() {
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
        assert_eq!(decode("00abff").unwrap(), [0x00, 0xab, 0xff]);
        assert_eq!(decode("00AbFF").unwrap(), [0x00, 0xab, 0xff]);
    }

    #[test]
    fn decode_rejects_odd_length() {
        assert!(matches!(
            decode("abc").unwrap_err().0,
            faster_hex_private::Error::InvalidLength(_)
        ));
    }

    #[test]
    fn decode_rejects_invalid_chars() {
        assert_eq!(
            decode("00gg").unwrap_err().0,
            faster_hex_private::Error::InvalidChar
        );
        assert_eq!(
            decode("0x00").unwrap_err().0,
            faster_hex_private::Error::InvalidChar
        );
    }
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{Cid, Codec, Error, Version};
use integer_encoding::VarIntReader;
use multihash::Multihash;
use std::convert::TryFrom;
use std::io::Cursor;
use std::str::FromStr;

impl TryFrom<String> for Cid {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Cid::try_from(value.as_str())
    }
}

impl TryFrom<&str> for Cid {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let decoded = decode_str(value)?;
        Cid::try_from(decoded)
    }
}

impl FromStr for Cid {
    type Err = Error;

    fn from_str(src: &str) -> Result<Self, Error> {
        Cid::try_from(src)
    }
}

impl TryFrom<Vec<u8>> for Cid {
    type Error = Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Cid::try_from(value.as_slice())
    }
}

impl TryFrom<&[u8]> for Cid {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if Version::is_v0_binary(value) {
            // Verify that hash can be decoded, this is very cheap
            let hash = Multihash::from_bytes(value.to_vec())?;
            Ok(Cid::new(Codec::DagCBOR, Version::V0, hash))
        } else {
            let (hash, version, codec) = decode_v1_bytes(value)?;
            // convert hash bytes to Multihash object
            let multihash = Multihash::from_bytes(hash)?;
            Ok(Cid::new(codec, version, multihash))
        }
    }
}

fn decode_v1_bytes(bz: &[u8]) -> Result<(Vec<u8>, Version, Codec), Error> {
    let mut cur = Cursor::new(bz);
    let raw_version = cur.read_varint()?;
    let raw_codec = cur.read_varint()?;

    let version = Version::from(raw_version)?;
    let codec = Codec::from(raw_codec)?;

    let hash = &bz[cur.position() as usize..];
    Ok((hash.to_vec(), version, codec))
}

fn decode_str(cid_str: &str) -> Result<Vec<u8>, Error> {
    static IPFS_DELIMETER: &str = "/ipfs/";

    let hash = match cid_str.find(IPFS_DELIMETER) {
        Some(index) => &cid_str[index + IPFS_DELIMETER.len()..],
        _ => cid_str,
    };

    if hash.len() < 2 {
        return Err(Error::InputTooShort);
    }

    let (_, decoded) = if Version::is_v0_str(hash) {
        // TODO: could avoid the roundtrip here and just use underlying
        // base-x base58btc decoder here.
        let hash = multibase::Base::Base58Btc.code().to_string() + hash;

        multibase::decode(hash)
    } else {
        multibase::decode(hash)
    }?;

    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use multihash::Code::Blake2b256;

    #[test]
    fn verify_base32_upper() {
        let t_str = "BAFY2BZACED2ESI3BXIMO7JEZGDJXKWPIU4VOM3RVG44CIENFSDGLLHEUIPHEE";
        let decoded = &decode_str(&t_str).unwrap();
        // decode bytes to into hash, version, codec and test intermediate values
        let (hash, version, codec) = decode_v1_bytes(&decoded).unwrap();
        assert_eq!(version, Version::V1, "failed version check");
        assert_eq!(codec, Codec::DagCBOR, "failed codec check");
        let hash = Multihash::from_bytes(hash.to_vec()).unwrap();
        assert_eq!(hash.algorithm(), Blake2b256);
    }
    #[test]
    fn verify_base32_lower() {
        let t_str = "bafy2bzaced2esi3bximo7jezgdjxkwpiu4vom3rvg44cienfsdgllheuiphee";
        let decoded = &decode_str(&t_str).unwrap();
        // decode bytes to into hash, version, codec and test intermediate values
        let (hash, version, codec) = decode_v1_bytes(&decoded).unwrap();
        assert_eq!(version, Version::V1, "failed version check");
        assert_eq!(codec, Codec::DagCBOR, "failed codec check");
        let hash = Multihash::from_bytes(hash.to_vec()).unwrap();
        assert_eq!(hash.algorithm(), Blake2b256);
    }
}

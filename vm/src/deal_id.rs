// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use unsigned_varint::decode::Error as UVarintError;

// TODO move DealID ref from other PR to here
pub type DealID = u64;

pub fn deal_key(d: DealID) -> String {
    let mut bz = unsigned_varint::encode::u64_buffer();
    unsigned_varint::encode::u64(d, &mut bz);
    String::from_utf8_lossy(&bz).to_string()
}

pub fn parse_uint_key(s: &str) -> Result<u64, UVarintError> {
    let (v, _) = unsigned_varint::decode::u64(s.as_ref())?;
    Ok(v)
}

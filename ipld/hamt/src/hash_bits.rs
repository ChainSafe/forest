// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{Error, HashedKey};
use std::cmp::Ordering;

/// Helper struct which indexes and allows returning bits from a hashed key
#[derive(Debug, Clone)]
pub struct HashBits<'a> {
    b: &'a HashedKey,
    pub consumed: u32,
}

#[inline]
fn mkmask(n: u32) -> u32 {
    ((1u64 << n) - 1) as u32
}

impl<'a> HashBits<'a> {
    pub fn new(hash_buffer: &'a HashedKey) -> HashBits<'a> {
        Self::new_at_index(hash_buffer, 0)
    }

    /// Constructs hash bits with custom consumed index
    pub fn new_at_index(hash_buffer: &'a HashedKey, consumed: u32) -> HashBits<'a> {
        Self {
            b: hash_buffer,
            consumed,
        }
    }

    /// Returns next `i` bits of the hash and returns the value as an integer and returns
    /// Error when maximum depth is reached
    pub fn next(&mut self, i: u32) -> Result<u32, Error> {
        if i > 8 {
            return Err(Error::InvalidHashBitLen);
        }
        if (self.consumed + i) as usize > self.b.len() * 8 {
            return Err(Error::MaxDepth);
        }
        Ok(self.next_bits(i))
    }

    fn next_bits(&mut self, i: u32) -> u32 {
        let curbi = self.consumed / 8;
        let leftb = 8 - (self.consumed % 8);

        let curb = self.b[curbi as usize] as u32;
        match i.cmp(&leftb) {
            Ordering::Equal => {
                // bits to consume is equal to the bits remaining in the currently indexed byte
                let out = mkmask(i) & curb;
                self.consumed += i;
                out
            }
            Ordering::Less => {
                // Consuming less than the remaining bits in the current byte
                let a = curb & mkmask(leftb);
                let b = a & !mkmask(leftb - i);
                let c = b >> (leftb - i);
                self.consumed += i;
                c
            }
            Ordering::Greater => {
                // Consumes remaining bits and remaining bits from a recursive call
                let mut out = (mkmask(leftb) & curb) as u64;
                out <<= i - leftb;
                self.consumed += leftb;
                out += self.next_bits(i - leftb) as u64;
                out as u32
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitfield() {
        let mut key: HashedKey = Default::default();
        key[0] = 0b10001000;
        key[1] = 0b10101010;
        key[2] = 0b10111111;
        key[3] = 0b11111111;
        let mut hb = HashBits::new(&key);
        // Test eq cmp
        assert_eq!(hb.next(8).unwrap(), 0b10001000);
        // Test lt cmp
        assert_eq!(hb.next(5).unwrap(), 0b10101);
        // Test gt cmp
        assert_eq!(hb.next(5).unwrap(), 0b01010);
        assert_eq!(hb.next(6).unwrap(), 0b111111);
        assert_eq!(hb.next(8).unwrap(), 0b11111111);
        assert!(matches!(hb.next(9), Err(Error::InvalidHashBitLen)));
        for _ in 0..28 {
            // Iterate through rest of key to test depth
            hb.next(8).unwrap();
        }
        assert!(matches!(hb.next(1), Err(Error::MaxDepth)));
    }
}

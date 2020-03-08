// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{Error, HashedKey};
use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct HashBits<'a> {
    b: &'a HashedKey,
    consumed: u8,
}

#[inline]
fn mkmask(n: u8) -> u8 {
    ((1u64 << n) - 1) as u8
}

impl<'a> HashBits<'a> {
    pub fn new(hash_buffer: &'a HashedKey) -> HashBits<'a> {
        Self {
            b: hash_buffer,
            consumed: 0,
        }
    }
    /// Returns next `i` bits of the hash and returns the value as an integer and returns
    /// Error when maximum depth is reached
    pub fn next(&mut self, i: u8) -> Result<u8, Error> {
        if (self.consumed + i) as usize > self.b.len() * 8 {
            return Err(Error::Custom("Maximum depth reached"));
        }
        Ok(self.next_bits(i))
    }

    fn next_bits(&mut self, i: u8) -> u8 {
        let curbi = self.consumed / 8;
        let leftb = 8 - (self.consumed % 8);

        let curb = self.b[curbi as usize];
        match i.cmp(&leftb) {
            Ordering::Equal => {
                let out = mkmask(i) & curb;
                self.consumed += i;
                out
            }
            Ordering::Less => {
                let a = curb & mkmask(leftb);
                let b = a & !mkmask(leftb - i);
                let c = b >> (leftb - i);
                self.consumed += i;
                c
            }
            Ordering::Greater => {
                let mut out = (mkmask(leftb) & curb) as u64;
                out <<= i - leftb;
                self.consumed += leftb;
                out += self.next_bits(i - leftb) as u64;
                out as u8
            }
        }
    }
}

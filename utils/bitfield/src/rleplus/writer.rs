// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[derive(Default)]
/// A `BitWriter` allows for efficiently writing bits to a byte buffer, up to a byte at a time.
pub struct BitWriter {
    /// The buffer that is written to.
    bytes: Vec<u8>,
    /// The most recently written bits. Whenever this exceeds 8 bits, one byte is written to `bytes`.
    bits: u16,
    /// The number of bits currently stored in `bits`.
    num_bits: u32,
}

impl BitWriter {
    /// Creates a new `BitWriter`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Writes a given number of bits from `byte` to the buffer.
    pub fn write(&mut self, byte: u8, num_bits: u32) {
        debug_assert!(num_bits <= 8);

        self.bits |= (byte as u16) << self.num_bits;
        self.num_bits += num_bits;

        // when we have a full byte in `self.bits`, we write it to `self.bytes`
        if self.num_bits >= 8 {
            self.bytes.push(self.bits as u8);
            self.bits >>= 8;
            self.num_bits -= 8;
        }
    }

    /// Writes a given length to the buffer according to RLE+ encoding.
    pub fn write_len(&mut self, len: usize) {
        debug_assert!(len > 0);

        if len == 1 {
            // Block Single (prefix 1)
            self.write(1, 1);
        } else if len < 16 {
            // Block Short (prefix 01)
            self.write(2, 2); // 2 == 01 with the least significant bit first
            self.write(len as u8, 4);
        } else {
            // Block Long (prefix 00)
            self.write(0, 2);

            let mut buffer = unsigned_varint::encode::usize_buffer();
            for &byte in unsigned_varint::encode::usize(len, &mut buffer) {
                self.write(byte, 8);
            }
        }
    }

    /// Writes any remaining bits to the buffer and returns it, as well as the number of
    /// padding zeros that were (possibly) added to fill the last byte.
    pub fn finish(mut self) -> (Vec<u8>, u32) {
        let padding = if self.num_bits > 0 {
            self.bytes.push(self.bits as u8);
            8 - self.num_bits
        } else {
            0
        };
        (self.bytes, padding)
    }
}

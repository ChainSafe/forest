// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Result;

// https://github.com/multiformats/unsigned-varint#practical-maximum-of-9-bytes-for-security
const VARINT_MAX_BYTES: usize = 9;

/// A `BitReader` allows for efficiently reading bits from a byte buffer, up to a byte at a time.
///
/// It works by always storing at least the next 8 bits in `bits`, which lets us conveniently
/// and efficiently read bits that cross a byte boundary. It's filled with the bits from `next_byte`
/// after every read operation, which is in turn replaced by the next byte from `bytes` as soon
/// as the next read might read bits from `next_byte`.
pub struct BitReader<'a> {
    /// The bytes that have not been read from yet.
    bytes: &'a [u8],
    /// The next byte from `bytes` to be added to `bits`.
    next_byte: u8,
    /// The next bits to be read.
    bits: u16,
    /// The number of bits in `bits` from bytes that came before `next_byte` (at least 8, at most 15).
    num_bits: u32,
}

impl<'a> BitReader<'a> {
    /// Creates a new `BitReader`.
    pub fn new(bytes: &'a [u8]) -> Self {
        let &byte1 = bytes.get(0).unwrap_or(&0);
        let &byte2 = bytes.get(1).unwrap_or(&0);
        let bytes = if bytes.len() > 2 { &bytes[2..] } else { &[] };

        Self {
            bytes,
            bits: byte1 as u16,
            next_byte: byte2,
            num_bits: 8,
        }
    }

    /// Reads a given number of bits from the buffer. Will keep returning 0 once
    /// the buffer has been exhausted.
    pub fn read(&mut self, num_bits: u32) -> u8 {
        debug_assert!(num_bits <= 8);

        // creates a mask with a `num_bits` number of 1s in order
        // to get only the bits we need from `self.bits`
        let mask = (1 << num_bits) - 1;
        let res = (self.bits & mask) as u8;

        // removes the bits we've just read from local storage
        // because we don't need them anymore
        self.bits >>= num_bits;
        self.num_bits -= num_bits;

        // this unconditionally adds the next byte to `bits`,
        // regardless of whether there's enough space or not. the
        // point is to make sure that `bits` always contains
        // at least the next 8 bits to be read
        self.bits |= (self.next_byte as u16) << self.num_bits;

        // if fewer than 8 bits remain, we increment `self.num_bits`
        // to include the bits from `next_byte` (which is already
        // contained in `bits`) and we update `next_byte` with the
        // data to be read after that
        if self.num_bits < 8 {
            self.num_bits += 8;

            let (&next_byte, bytes) = self.bytes.split_first().unwrap_or((&0, &[]));
            self.next_byte = next_byte;
            self.bytes = bytes;
        }

        res
    }

    /// Reads a varint from the buffer. Returns an error if the
    /// current position on the buffer contains no valid varint.
    fn read_varint(&mut self) -> Result<usize> {
        let mut len = 0;

        for i in 0..VARINT_MAX_BYTES {
            let byte = self.read(8);

            // strip off the most significant bit and add
            // it to the output
            len |= (byte as usize & 0x7f) << (i * 7);

            // if the most significant bit is a 0, we've
            // reached the end of the varint
            if byte & 0x80 == 0 {
                return Ok(len);
            }
        }

        Err("Invalid varint")
    }

    /// Reads a length from the buffer according to RLE+ encoding.
    pub fn read_len(&mut self) -> Result<Option<usize>> {
        let prefix_0 = self.read(1);

        let len = if prefix_0 == 1 {
            // Block Single (prefix 1)
            1
        } else {
            let prefix_1 = self.read(1);

            if prefix_1 == 1 {
                // Block Short (prefix 01)
                self.read(4) as usize
            } else {
                // Block Long (prefix 00)
                self.read_varint()?
            }
        };

        // decoding ends when a length of 0 is encountered, regardless of
        // whether it is a short block or a long block
        Ok(if len > 0 { Some(len) } else { None })
    }
}

#[cfg(test)]
mod tests {
    use super::BitReader;

    #[test]
    fn read() {
        let bytes = &[0b1011_1110, 0b0111_0010, 0b0010_1010];
        let mut reader = BitReader::new(bytes);

        assert_eq!(reader.read(0), 0);
        assert_eq!(reader.read(1), 0);
        assert_eq!(reader.read(3), 0b111);
        assert_eq!(reader.read(6), 0b101011);
        assert_eq!(reader.read(1), 0);
        assert_eq!(reader.read(4), 0b1110);
        assert_eq!(reader.read(3), 0b100);
        assert_eq!(reader.read(2), 0b10);
        assert_eq!(reader.read(3), 0b010);
        assert_eq!(reader.read(4), 0);
        assert_eq!(reader.read(8), 0);
        assert_eq!(reader.read(0), 0);
    }

    #[test]
    fn read_len() {
        let bytes = &[0b0001_0101, 0b1101_0111, 0b0110_0111, 0b00110010];
        let mut reader = BitReader::new(bytes);

        assert_eq!(reader.read_len().unwrap(), Some(1)); // prefix: 1
        assert_eq!(reader.read_len().unwrap(), Some(2)); // prefix: 01, value: 0100 (LSB to MSB)
        assert_eq!(reader.read_len().unwrap(), Some(11)); // prefix: 01, value: 1101
        assert_eq!(reader.read_len().unwrap(), Some(15)); // prefix: 01, value: 1111
        assert_eq!(reader.read_len().unwrap(), Some(147)); // prefix: 00, value: 11001001 10000000
        assert_eq!(reader.read_len().unwrap(), None);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "assertion failed")]
    fn too_many_bits_at_once() {
        let mut reader = BitReader::new(&[]);
        reader.read(16);
    }

    #[test]
    fn roundtrip() {
        use super::super::BitWriter;
        use rand::{Rng, SeedableRng};
        use rand_xorshift::XorShiftRng;

        let mut rng = XorShiftRng::seed_from_u64(5);

        for _ in 0..100 {
            let lengths: Vec<_> = std::iter::repeat_with(|| rng.gen_range(1, 200))
                .take(100)
                .collect();

            let mut writer = BitWriter::new();

            for &len in &lengths {
                writer.write_len(len);
            }

            let bytes = writer.finish();
            let mut reader = BitReader::new(&bytes);

            for &len in &lengths {
                assert_eq!(reader.read_len().unwrap(), Some(len));
            }

            assert_eq!(reader.read_len().unwrap(), None);
        }
    }
}

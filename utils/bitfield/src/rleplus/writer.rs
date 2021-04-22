// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[derive(Default, Clone, Debug)]
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
        debug_assert!(8 - byte.leading_zeros() <= num_bits);

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

    /// Writes any remaining bits to the buffer and returns it.
    pub fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            self.bytes.push(self.bits as u8);
        }

        // This check should not be necessary, but as a sanity check to make sure 0 bytes
        // aren't added at the end of the bytes
        while let Some(0) = self.bytes.last() {
            self.bytes.pop();
        }
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::BitWriter;

    #[test]
    fn write() {
        let mut writer = BitWriter::new();
        let empty_vec: Vec<u8> = Vec::new();
        assert_eq!(writer.clone().finish(), empty_vec);

        // Trailing 0 bits are not included
        writer.write(0b0000_0000, 4);
        assert_eq!(writer.clone().finish(), &[] as &[u8]);

        writer.write(0b0000_0000, 4);
        assert_eq!(writer.clone().finish(), &[] as &[u8]);

        writer.write(0b0000_0001, 4);
        assert_eq!(writer.clone().finish(), &[0b0000_0000, 0b0000_0001]);
        //                                                        ^^^^

        writer.write(0b0000_0011, 2);
        assert_eq!(writer.clone().finish(), &[0b0000_0000, 0b0011_0001]);
        //                                                     ^^

        writer.write(0b0000_0110, 3);
        assert_eq!(
            writer.clone().finish(),
            &[0b0000_0000, 0b1011_0001, 0b0000_0001]
        ); //                ^^                   ^

        writer.write(0b0111_0100, 8);
        assert_eq!(writer.finish(), &[0b0000_0000, 0b1011_0001, 0b1110_1001]);
        //                                                        ^^^^ ^^^
    }

    #[test]
    fn write_len() {
        let mut writer = BitWriter::new();

        writer.write_len(1); // prefix: 1
        assert_eq!(writer.clone().finish(), &[0b0000_0001]);
        //                                              ^

        writer.write_len(2); // prefix: 01, value: 0100 (LSB to MSB)
        assert_eq!(writer.clone().finish(), &[0b0001_0101]);
        //                                       ^^^ ^^^

        writer.write_len(11); // prefix: 01, value: 1101
        assert_eq!(writer.clone().finish(), &[0b0001_0101, 0b0001_0111]);
        //                                      ^               ^ ^^^^

        writer.write_len(15); // prefix: 01, value: 1111
        assert_eq!(
            writer.clone().finish(),
            &[0b0001_0101, 0b1101_0111, 0b0000_0111]
        ); //                ^^^                ^^^

        writer.write_len(147); // prefix: 00, value: 11001001 10000000
        assert_eq!(
            writer.finish(),
            &[
                0b0001_0101,
                0b1101_0111,
                0b0110_0111,
                //^^^^ ^
                0b0011_0010,
                //^^^^ ^^^^
            ]
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "assertion failed")]
    fn zero_len() {
        let mut writer = BitWriter::new();
        writer.write_len(0);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "assertion failed")]
    fn more_bits_than_indicated() {
        let mut writer = BitWriter::new();
        writer.write(100, 0);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "assertion failed")]
    fn too_many_bits_at_once() {
        let mut writer = BitWriter::new();
        writer.write(0, 16);
    }
}

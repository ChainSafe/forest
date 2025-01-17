use std::io;

use futures::{AsyncRead, AsyncReadExt};

// Max size of u64 varint
const U64_LEN: usize = 10;

pub(crate) async fn read_varint_u64<R: AsyncRead + Unpin>(
    stream: &mut R,
) -> Result<Option<(u64, usize)>, io::Error> {
    let mut result: u64 = 0;

    for i in 0..U64_LEN {
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf).await?;

        let byte = buf[0];
        result |= u64::from(byte & 0b0111_1111) << (i * 7);

        // If is last byte = leftmost bit is zero
        if byte & 0b1000_0000 == 0 {
            return Ok(Some((result, i + 1)));
        }
    }

    // TODO: Return error
    Ok(None)
}

#[cfg(test)]
mod tests {
    use futures::io::Cursor;
    use quickcheck_macros::quickcheck;

    use super::super::varint::read_varint_u64;
    use super::U64_LEN;

    // Implementation copied from https://github.com/paritytech/unsigned-varint/blob/a3a5b8f2bee1f44270629e96541adf805a53d32c/src/encode.rs#L22
    fn encode_varint_u64(input: u64, buf: &mut [u8; U64_LEN]) -> (&[u8], usize) {
        let mut n = input;
        let mut i = 0;
        for b in buf.iter_mut() {
            *b = n as u8 | 0b1000_0000;
            n >>= 7;
            if n == 0 {
                *b &= 0b0111_1111;
                break;
            }
            i += 1
        }
        debug_assert_eq!(n, 0);
        (&buf[0..=i], i + 1)
    }

    // quickcheck macro constructs a test function that runs the assertion below with random inputs,
    // and attempting to find counter examples efficiently
    #[quickcheck]
    fn varint_u64_identity(input: u64) -> bool {
        let mut buf = [0u8; U64_LEN];
        let (buf_ref, input_len) = encode_varint_u64(input, &mut buf);

        // Cursor = easy way to get AsyncRead from an AsRef<[u8]>
        let mut input_stream = Cursor::new(buf_ref);
        // Run async task blocking to preserve quickcheck mechanics
        let output = futures::executor::block_on(read_varint_u64(&mut input_stream));

        (input, input_len) == output.unwrap().unwrap()
    }
}

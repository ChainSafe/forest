use positioned_io::{ReadAt, Size};
use std::{cmp, io};

/// Like [`std::num::NonZeroU64`], but is never [`u64::MAX`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NonMaximalU64(u64);

impl NonMaximalU64 {
    pub fn new(u: u64) -> Option<Self> {
        match u == u64::MAX {
            true => None,
            false => Some(Self(u)),
        }
    }
    pub fn fit(u: u64) -> Self {
        Self(u.saturating_sub(1))
    }
    pub fn get(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for NonMaximalU64 {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self::fit(u64::arbitrary(g))
    }
}

/// A view of at most [`Self::limit`] bytes, starting at [`Self::offset`] in [`Self::io`].
/// [`positioned_io::Slice`] with a less suprising [`Size`] implementation.
#[derive(Debug, Clone)]
pub struct Slice<I> {
    pub io: I,
    pub offset: u64,
    pub limit: Option<u64>,
}

impl<I> Slice<I> {
    pub fn new(io: I, offset: u64, limit: impl Into<Option<u64>>) -> Self {
        Slice {
            io,
            offset,
            limit: limit.into(),
        }
    }

    /// Maybe limit the number of bytes to be read at the given position
    ///
    /// ```text
    ///      limit
    ///      ----------->
    /// xxxx|------------|xxxx
    /// --->
    /// offset
    ///      ---->
    ///        pos
    /// ```
    ///
    /// # Panics
    /// - if `request` is greater than [`u64::MAX`].
    /// - if `self.limit - pos` is greater than [`usize::MAX`]
    fn maybe_limit(&self, pos: u64, request: usize) -> usize {
        match self.limit {
            None => request,
            Some(limit) => usize::try_from(cmp::min(
                u64::try_from(request).unwrap(),
                limit.saturating_sub(pos),
            ))
            .unwrap(),
        }
    }
}

impl<I: ReadAt> ReadAt for Slice<I> {
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        let bytes = self.maybe_limit(pos, buf.len());
        self.io.read_at(pos + self.offset, &mut buf[..bytes])
    }
}

impl<I: Size> Size for Slice<I> {
    fn size(&self) -> io::Result<Option<u64>> {
        match (self.io.size()?, self.limit) {
            (Some(underlying), Some(limit)) => Ok(Some(cmp::min(
                underlying.saturating_sub(self.offset),
                limit,
            ))),
            (Some(underlying), None) => Ok(Some(underlying.saturating_sub(self.offset))),
            (None, _) => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let io: &[u8] = &[0, 1, 2];
        do_check(&[0, 1, 2], Slice::new(io, 0, None));
        do_check(&[1, 2], Slice::new(io, 1, None));
        do_check(&[0, 1], Slice::new(io, 0, 2));
        do_check(&[1], Slice::new(io, 1, 1));
    }

    #[track_caller]
    fn do_check(expected: &[u8], subject: Slice<&[u8]>) {
        assert_eq!(
            expected.len(),
            subject.size().unwrap().unwrap() as usize,
            "size mismatch"
        );
        let mut buf = vec![0; subject.io.len()];
        let n = subject.read_at(0, &mut buf).unwrap();
        buf.truncate(n);
        assert_eq!(expected, buf, "contents mismatch");
    }
}

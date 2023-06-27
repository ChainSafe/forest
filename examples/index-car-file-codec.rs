//! Don't use this - it will read each block into memory, which
//! - we don't need
//! - costs a lot of perf

use anyhow::Context as _;
use bytes::Bytes;
use clap::Parser;
use futures_util::{Stream, StreamExt, TryStreamExt};
use fvm_ipld_car::CarHeader;
use pin_project_lite::pin_project;
use std::{
    path::PathBuf,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncSeek, AsyncSeekExt},
};
use tokio_util_06::codec::{Decoder, FramedRead};
use unsigned_varint::codec::UviBytes;

#[derive(Parser)]
struct Args {
    path: PathBuf,
}
pin_project! {
    struct FramedReadWithPosition<T, D> {
        #[pin]
        framed_read: FramedRead<T, D>,
    }
}

impl<T, D> FramedReadWithPosition<T, D> {
    fn new(framed_read: FramedRead<T, D>) -> Self {
        Self { framed_read }
    }
}

impl<T, D> Stream for FramedReadWithPosition<T, D>
where
    T: AsyncRead + AsyncSeek + Unpin,
    D: Decoder,
{
    type Item = Result<(u64, D::Item), D::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use futures::Future as _;
        let this = self.project();
        std::pin::pin!(next_with_position(this.framed_read)).poll(cx)
    }
}

async fn next_with_position<T, D>(
    frames: Pin<&mut FramedRead<T, D>>,
) -> Option<Result<(u64, D::Item), D::Error>>
where
    T: AsyncSeek + AsyncRead + Unpin,
    D: Decoder,
{
    // we're just about to read an item - what position will the stream be?
    let frames = frames.get_mut();
    let position_of_reader = match frames.get_mut().stream_position().await {
        Ok(p) => p,
        Err(e) => return Some(Err(D::Error::from(e))),
    };
    let bytes_in_buf = u64::try_from(frames.read_buffer().len()).unwrap();
    let position = position_of_reader - bytes_in_buf;

    // now read the frame
    match frames.next().await? {
        Ok(o) => Some(Ok((position, o))),
        Err(e) => Some(Err(e)),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Args { path } = Args::parse();
    let mut framed_read = FramedRead::new(File::open(path).await?, UviBytes::<Bytes>::default());
    let header = framed_read
        .next()
        .await
        .context("no header")?
        .context("error getting header")?;
    let header = fvm_ipld_encoding::from_slice::<CarHeader>(&header).context("invalid header")?;
    println!("header has {} roots", header.roots.len());
    let frames = FramedReadWithPosition::new(framed_read)
        .map_ok(|(_offset, frame)| {
            let len = frame.len();
            println!("frame of len {len}");
            len
        })
        .try_collect::<Vec<_>>()
        .await?;
    println!("{}", frames.len());
    Ok(())
}

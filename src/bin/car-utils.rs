use bytes::{Buf as _, BufMut, BytesMut};
use futures_util::{Stream, StreamExt as _, TryStream, TryStreamExt as _};
use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use pin_project_lite::pin_project;
use std::{
    ops::ControlFlow,
    path::PathBuf,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio::fs::File;
use tokio_util_06::codec::{FramedRead, FramedWrite};
use tracing::debug;
use tracing_subscriber::EnvFilter;

type VarintFrameCodec = unsigned_varint::codec::UviBytes<BytesMut>;

#[derive(Debug, clap::Parser)]
enum Args {
    AggregateVarintFramesInZstdFrames {
        source: PathBuf,
        destination: PathBuf,
        #[arg(short, long, default_value_t = 3)]
        compression_level: u16,
        /// Uncompressed, not including varint frame header
        #[arg(short, long, default_value_t = 8000usize.next_power_of_two())]
        uncompressed_data_per_zstd_frame: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    _main(clap::Parser::parse()).await
}

async fn _main(args: Args) -> anyhow::Result<()> {
    debug!(?args);
    match args {
        Args::AggregateVarintFramesInZstdFrames {
            source,
            destination,
            compression_level,
            uncompressed_data_per_zstd_frame,
        } => {
            let progress = MultiProgress::new();

            let varint_frame_count = progress.add(
                ProgressBar::new_spinner()
                    .with_style(count())
                    .with_message("varint frames")
                    .with_finish(ProgressFinish::AndLeave),
            );
            let zstd_frame_count = progress.add(
                ProgressBar::new_spinner()
                    .with_style(count())
                    .with_message("zstd frames")
                    .with_finish(ProgressFinish::AndLeave),
            );

            let source = File::open(source).await?;
            let source = progress
                .add(
                    ProgressBar::new(source.metadata().await?.len())
                        .with_style(read())
                        .with_message("reading"),
                )
                .wrap_async_read(source);

            let source = FramedRead::new(source, VarintFrameCodec::default());
            let destination = FramedWrite::new(
                progress
                    .add(
                        ProgressBar::new_spinner()
                            .with_style(write())
                            .with_message("written")
                            .with_finish(ProgressFinish::AndLeave),
                    )
                    .wrap_async_write(File::create(destination).await?),
                VarintFramesToZstdFrame {
                    compression_level,
                    varint_frame_codec: VarintFrameCodec::default(),
                },
            );
            try_collate(
                source.inspect_ok(|_| varint_frame_count.inc(1)),
                |varint_frames, next_varint_frame| {
                    let collated_len = varint_frames.iter().map(BytesMut::len).sum::<usize>();
                    match collated_len + next_varint_frame.len() > uncompressed_data_per_zstd_frame
                    {
                        true => ControlFlow::Break(()),
                        false => ControlFlow::Continue(()),
                    }
                },
            )
            .inspect_ok(|_| zstd_frame_count.inc(1))
            .forward(destination)
            .await?;
        }
    }
    Ok(())
}

fn try_collate<TryStreamT, OkT, ErrT, ShouldCollateFn>(
    inner: TryStreamT,
    should_collate_fn: ShouldCollateFn,
) -> TryCollate<TryStreamT, OkT, ShouldCollateFn>
where
    ShouldCollateFn: FnMut(&[OkT], &OkT) -> ControlFlow<(), ()>,
    TryStreamT: TryStream<Ok = OkT, Error = ErrT>,
{
    TryCollate {
        inner,
        current_collation: vec![],
        should_collate_fn,
    }
}

pin_project! {
    struct TryCollate<Inner, InnerOk, ShouldCollateFn> {
        #[pin]
        inner: Inner,
        current_collation: Vec<InnerOk>,
        should_collate_fn: ShouldCollateFn
    }
}

impl<Inner, InnerOk, ShouldCollateFn> Stream for TryCollate<Inner, InnerOk, ShouldCollateFn>
where
    Inner: TryStream<Ok = InnerOk>,
    ShouldCollateFn: FnMut(&[InnerOk], &InnerOk) -> ControlFlow<(), ()>,
{
    type Item = Result<Vec<InnerOk>, Inner::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();
        loop {
            match ready!(this.inner.as_mut().try_poll_next(cx)) {
                Some(Ok(ok)) => match (this.should_collate_fn)(this.current_collation, &ok) {
                    // collate it
                    ControlFlow::Continue(_) => {
                        this.current_collation.push(ok);
                    }
                    // return what we've got, and start a new collation
                    ControlFlow::Break(_) => {
                        let collation = std::mem::take(this.current_collation);
                        this.current_collation.push(ok);
                        return Poll::Ready(Some(Ok(collation)));
                    }
                },
                // ordering between errors and collated types is broken here
                Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                None => {
                    let last = match this.current_collation.is_empty() {
                        true => None,
                        false => Some(Ok(std::mem::take(this.current_collation))),
                    };
                    return Poll::Ready(last);
                }
            }
        }
    }
}

#[test]
fn test_roundtrip() {
    let uncompressed = include_bytes!("../../test-snapshots/chain4.car");
    let mut compressed = vec![];

    futures::executor::block_on(
        try_collate(
            FramedRead::new(uncompressed.as_slice(), VarintFrameCodec::default()),
            |varint_frames, next_varint_frame| {
                let collated_len = varint_frames.iter().map(BytesMut::len).sum::<usize>();
                match collated_len + next_varint_frame.len() > 4096 {
                    true => ControlFlow::Break(()),
                    false => ControlFlow::Continue(()),
                }
            },
        )
        .forward(FramedWrite::new(
            &mut compressed,
            VarintFramesToZstdFrame {
                compression_level: 3,
                varint_frame_codec: VarintFrameCodec::default(),
            },
        )),
    )
    .unwrap();

    let round_tripped = zstd::decode_all(compressed.as_slice()).unwrap();
    assert!(round_tripped == uncompressed);
}

#[tokio::test]
async fn test_collate() {
    let source = futures::stream::iter(["hello", "my", "name", "is", "aaaaaaaaaatif"])
        .map(Ok)
        .chain(futures::stream::iter([Err(())]));
    let mut collated = try_collate(source, |collated, next| {
        let collated_len = collated.iter().map(|it| it.len()).sum::<usize>();
        match collated_len + next.len() > 10 {
            true => ControlFlow::Break(()),
            false => ControlFlow::Continue(()),
        }
    });
    assert_eq!(collated.next().await, Some(Ok(vec!["hello", "my"])));
    assert_eq!(collated.next().await, Some(Ok(vec!["name", "is"])));
    assert_eq!(collated.next().await, Some(Err(())));
    assert_eq!(collated.next().await, Some(Ok(vec!["aaaaaaaaaatif"]))); // odd, but fine
    assert_eq!(collated.next().await, None);
}

struct VarintFramesToZstdFrame {
    compression_level: u16,
    varint_frame_codec: VarintFrameCodec,
}

impl tokio_util_06::codec::Encoder<Vec<BytesMut>> for VarintFramesToZstdFrame {
    type Error = std::io::Error;

    fn encode(
        &mut self,
        buffers: Vec<BytesMut>,
        dst: &mut bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        let mut uncompressed = BytesMut::with_capacity(buffers.iter().map(BytesMut::len).sum());
        for buffer in buffers {
            self.varint_frame_codec.encode(buffer, &mut uncompressed)?;
        }

        zstd::stream::copy_encode(
            uncompressed.reader(),
            dst.writer(),
            i32::from(self.compression_level),
        )
    }
}

fn read() -> ProgressStyle {
    ProgressStyle::with_template(
        "{msg:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
    )
    .expect("invalid progress template")
    .progress_chars("=>-")
}

const TICK_STRINGS: &[&str] = &[
    "▹▹▹▹▹",
    "▸▹▹▹▹",
    "▹▸▹▹▹",
    "▹▹▸▹▹",
    "▹▹▹▸▹",
    "▹▹▹▹▸",
    "▪▪▪▪▪",
];

fn write() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.blue} {bytes} {msg}")
        .unwrap()
        .tick_strings(TICK_STRINGS)
}

fn count() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.blue} {pos} {msg}")
        .unwrap()
        .tick_strings(TICK_STRINGS)
}

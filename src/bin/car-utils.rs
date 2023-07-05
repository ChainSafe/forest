use bytes::{buf::Writer, BufMut as _, BytesMut};
use futures_util::{Stream, StreamExt as _, TryStream, TryStreamExt as _};
use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use pin_project_lite::pin_project;
use std::{
    io::Write as _,
    marker::PhantomData,
    ops::ControlFlow,
    path::PathBuf,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedWrite};
use tokio_util_06::codec::FramedRead;
use tracing::debug;
use tracing_subscriber::EnvFilter;
use zstd::Encoder;

type VarintFrameCodec = unsigned_varint::codec::UviBytes<BytesMut>;

#[derive(Debug, clap::Parser)]
enum Args {
    /// Each zstd frame will contain a whole number of varint frames
    AggregateVarintFramesInZstdFrames {
        source: PathBuf,
        destination: PathBuf,
        #[arg(short, long, default_value_t = 3)]
        compression_level: u16,
        /// End zstd frames after they exceed this length
        #[arg(short, long, default_value_t = 8000usize.next_power_of_two())]
        zstd_frame_length_tripwire: usize,
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
            zstd_frame_length_tripwire,
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
                BytesCodec::new(),
            );

            try_collate(
                source.inspect_ok(|_| varint_frame_count.inc(1)),
                fold_varint_bodies_into_zstd_frames(zstd_frame_length_tripwire, compression_level),
                finish_zstd_frame,
            )
            .inspect_ok(|_| zstd_frame_count.inc(1))
            .forward(destination)
            .await?;
        }
    }
    Ok(())
}

fn fold_varint_bodies_into_zstd_frames(
    tripwire: usize,
    compression_level: u16,
) -> impl Fn(
    Collate<Encoder<'_, Writer<BytesMut>>, BytesMut>,
) -> ControlFlow<BytesMut, Encoder<'_, Writer<BytesMut>>> {
    move |collate| {
        let encoder = match collate {
            Collate::Started(body) => write_varint_frame(
                Encoder::new(BytesMut::new().writer(), i32::from(compression_level)).unwrap(),
                body,
            ),
            Collate::Continued(encoder, body) => write_varint_frame(encoder, body),
        };
        let compressed_len = encoder.get_ref().get_ref().len();

        match compressed_len >= tripwire {
            // finish this zstd frame
            true => ControlFlow::Break(finish_zstd_frame(encoder)),
            // fold the next varint frame body in
            false => ControlFlow::Continue(encoder),
        }
    }
}

fn finish_zstd_frame(encoder: Encoder<Writer<BytesMut>>) -> BytesMut {
    encoder
        .finish()
        .expect("BytesMut has infallible IO")
        .into_inner()
}

fn write_varint_frame(
    mut encoder: Encoder<Writer<BytesMut>>,
    body: BytesMut,
) -> Encoder<Writer<BytesMut>> {
    let mut header = unsigned_varint::encode::usize_buffer();
    encoder
        .write_all(unsigned_varint::encode::usize(body.len(), &mut header))
        .expect("BytesMut has infallible IO");
    encoder
        .write_all(&body)
        .expect("BytesMut has infallible IO");
    encoder
}

fn try_collate<Inner, Collator, CollateFn, FinishFn, Collated>(
    inner: Inner,
    collate_fn: CollateFn,
    finish_fn: FinishFn,
) -> TryCollate<Inner, Collator, CollateFn, FinishFn, Collated>
where
    Inner: TryStream,
    CollateFn: FnMut(Collate<Collator, Inner::Ok>) -> ControlFlow<Collated, Collator>,
    FinishFn: FnMut(Collator) -> Collated,
{
    TryCollate {
        inner,
        collator: None,
        collate_fn,
        finish_fn,
        collated: PhantomData,
    }
}

pin_project! {
    struct TryCollate<Inner, Collator, CollateFn, FinishFn, Collated> {
        #[pin]
        inner: Inner,
        collator: Option<Collator>,
        collate_fn: CollateFn,
        finish_fn: FinishFn,
        collated: PhantomData<Collated>
    }
}

enum Collate<Collator, Item> {
    /// Handle the first item since the last collation
    Started(Item),
    /// Fold into the existing collator
    Continued(Collator, Item),
}

impl<Inner, Collator, CollateFn, FinishFn, Collated> Stream
    for TryCollate<Inner, Collator, CollateFn, FinishFn, Collated>
where
    Inner: TryStream,
    CollateFn: FnMut(Collate<Collator, Inner::Ok>) -> ControlFlow<Collated, Collator>,
    FinishFn: FnMut(Collator) -> Collated,
{
    type Item = Result<Collated, Inner::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();
        loop {
            match ready!(this.inner.as_mut().try_poll_next(cx)) {
                Some(Ok(ok)) => {
                    let action = match this.collator.take() {
                        Some(collator) => (this.collate_fn)(Collate::Continued(collator, ok)),
                        None => (this.collate_fn)(Collate::Started(ok)),
                    };
                    match action {
                        ControlFlow::Continue(collator) => *this.collator = Some(collator),
                        ControlFlow::Break(collated) => break Poll::Ready(Some(Ok(collated))),
                    }
                }
                Some(Err(error)) => break Poll::Ready(Some(Err(error))),
                None => match this.collator.take() {
                    Some(collator) => break Poll::Ready(Some(Ok((this.finish_fn)(collator)))),
                    None => break Poll::Ready(None),
                },
            }
        }
    }
}

#[tokio::test]
async fn test_try_collate() {
    let source = futures::stream::iter(["the", "cuttlefish", "is", "not", "a", "fish"])
        .map(Ok)
        .chain(futures::stream::iter([Err(())]));

    let mut collated = try_collate(
        source,
        |request| {
            let buffer = match request {
                Collate::Started(el) => String::from(el),
                Collate::Continued(already, el) => already + el,
            };
            match buffer.len() >= 5 {
                true => ControlFlow::Break(buffer),
                false => ControlFlow::Continue(buffer),
            }
        },
        std::convert::identity,
    );

    assert_eq!(collated.next().await.unwrap().unwrap(), "thecuttlefish");
    assert_eq!(collated.next().await.unwrap().unwrap(), "isnot");
    assert_eq!(collated.next().await.unwrap().unwrap(), "afish");
    collated.next().await.unwrap().unwrap_err();
    assert!(collated.next().await.is_none());
}

#[test]
fn test_roundtrip() {
    let uncompressed = include_bytes!("../../test-snapshots/chain4.car");
    let mut compressed = vec![];

    futures::executor::block_on(
        try_collate(
            FramedRead::new(uncompressed.as_slice(), VarintFrameCodec::default()),
            fold_varint_bodies_into_zstd_frames(4096, 3),
            finish_zstd_frame,
        )
        .forward(FramedWrite::new(&mut compressed, BytesCodec::new())),
    )
    .unwrap();

    assert!(compressed.len() < uncompressed.len());

    let round_tripped = zstd::decode_all(compressed.as_slice()).unwrap();
    assert!(round_tripped == uncompressed);
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

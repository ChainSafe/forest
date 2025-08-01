// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use flume::r#async::RecvStream;
use futures::{Stream, stream::FusedStream, task::Poll};
use get_size2::GetSize;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::gauge::Gauge,
    registry::Registry,
    registry::Unit,
};
use std::{
    borrow::Cow,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::Context,
};

use crate::metrics::default_registry;

pub trait FlumeSenderExt<T> {
    fn send_or_warn(&self, msg: T);
}

impl<T> FlumeSenderExt<T> for flume::Sender<T> {
    fn send_or_warn(&self, msg: T) {
        if let Err(e) = self.send(msg) {
            tracing::warn!("{e}");
        }
    }
}

pub fn unbounded_with_default_metrics_registry<T>(
    name: Cow<'static, str>,
) -> (SizeTrackingSender<T>, SizeTrackingReceiver<T>) {
    unbounded_with_metrics_registry(name, &mut default_registry())
}

pub fn unbounded_with_metrics_registry<T>(
    name: Cow<'static, str>,
    registry: &mut Registry,
) -> (SizeTrackingSender<T>, SizeTrackingReceiver<T>) {
    let (sender, receiver) = flume::unbounded();
    new_with_metrics_registry(name, registry, sender, receiver, None)
}

pub fn bounded_with_default_metrics_registry<T>(
    capacity: usize,
    name: Cow<'static, str>,
) -> (SizeTrackingSender<T>, SizeTrackingReceiver<T>) {
    bounded_with_metrics_registry(capacity, name, &mut default_registry())
}

pub fn bounded_with_metrics_registry<T>(
    capacity: usize,
    name: Cow<'static, str>,
    registry: &mut Registry,
) -> (SizeTrackingSender<T>, SizeTrackingReceiver<T>) {
    let (sender, receiver) = flume::bounded(capacity);
    new_with_metrics_registry(name, registry, sender, receiver, Some(capacity))
}

fn new_with_metrics_registry<T>(
    name: Cow<'static, str>,
    registry: &mut Registry,
    sender: flume::Sender<T>,
    receiver: flume::Receiver<T>,
    capacity: Option<usize>,
) -> (SizeTrackingSender<T>, SizeTrackingReceiver<T>) {
    static ID_GENERATOR: AtomicUsize = AtomicUsize::new(0);

    let id = ID_GENERATOR.fetch_add(1, Ordering::Relaxed);
    let tracker = ChannelMemoryUsageTracker {
        id,
        name,
        capacity,
        ..Default::default()
    };
    registry.register_collector(Box::new(tracker.clone()));

    (
        SizeTrackingSender {
            sender,
            tracker: tracker.clone(),
        },
        SizeTrackingReceiver { receiver, tracker },
    )
}

#[derive(Debug)]
pub struct SizeTrackingSender<T> {
    sender: flume::Sender<T>,
    tracker: ChannelMemoryUsageTracker,
}

impl<T> Clone for SizeTrackingSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            tracker: self.tracker.clone(),
        }
    }
}

impl<T: GetSize> SizeTrackingSender<T> {
    pub fn send(&self, msg: T) -> Result<(), flume::SendError<T>> {
        let size = msg.get_size();
        self.sender.send(msg)?;
        self.tracker.queued_len.fetch_add(1, Ordering::Relaxed);
        self.tracker.total_len.fetch_add(1, Ordering::Relaxed);
        self.tracker
            .queued_size_in_bytes
            .fetch_add(size, Ordering::Relaxed);
        self.tracker
            .total_size_in_bytes
            .fetch_add(size, Ordering::Relaxed);
        Ok(())
    }

    pub async fn send_async(&self, msg: T) -> Result<(), flume::SendError<T>> {
        let size = msg.get_size();
        self.sender.send_async(msg).await?;
        self.tracker.queued_len.fetch_add(1, Ordering::Relaxed);
        self.tracker.total_len.fetch_add(1, Ordering::Relaxed);
        self.tracker
            .queued_size_in_bytes
            .fetch_add(size, Ordering::Relaxed);
        self.tracker
            .total_size_in_bytes
            .fetch_add(size, Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Debug)]
pub struct SizeTrackingReceiver<T> {
    receiver: flume::Receiver<T>,
    tracker: ChannelMemoryUsageTracker,
}

impl<T> Clone for SizeTrackingReceiver<T> {
    fn clone(&self) -> Self {
        Self {
            receiver: self.receiver.clone(),
            tracker: self.tracker.clone(),
        }
    }
}

impl<T: GetSize> SizeTrackingReceiver<T> {
    #[allow(dead_code)]
    pub fn recv(&self) -> Result<T, flume::RecvError> {
        let msg = self.receiver.recv()?;
        let size = msg.get_size();
        self.tracker.queued_len.fetch_sub(1, Ordering::Relaxed);
        self.tracker
            .queued_size_in_bytes
            .fetch_sub(size, Ordering::Relaxed);
        Ok(msg)
    }

    pub async fn recv_async(&self) -> Result<T, flume::RecvError> {
        let msg = self.receiver.recv_async().await?;
        let size = msg.get_size();
        self.tracker.queued_len.fetch_sub(1, Ordering::Relaxed);
        self.tracker
            .queued_size_in_bytes
            .fetch_sub(size, Ordering::Relaxed);
        Ok(msg)
    }

    pub fn stream(&self) -> SizeTrackingRecvStream<'_, T> {
        SizeTrackingRecvStream {
            stream: self.receiver.stream(),
            tracker: self.tracker.clone(),
        }
    }
}

pub struct SizeTrackingRecvStream<'a, T> {
    stream: RecvStream<'a, T>,
    tracker: ChannelMemoryUsageTracker,
}

impl<'a, T: GetSize> Stream for SizeTrackingRecvStream<'a, T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(item)) => {
                self.tracker.queued_len.fetch_sub(1, Ordering::Relaxed);
                self.tracker
                    .queued_size_in_bytes
                    .fetch_sub(item.get_size(), Ordering::Relaxed);
                Poll::Ready(Some(item))
            }
        }
    }
}

impl<'a, T: GetSize> FusedStream for SizeTrackingRecvStream<'a, T> {
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

#[derive(Debug, Clone, Default)]
struct ChannelMemoryUsageTracker {
    id: usize,
    name: Cow<'static, str>,
    total_size_in_bytes: Arc<AtomicUsize>,
    queued_size_in_bytes: Arc<AtomicUsize>,
    total_len: Arc<AtomicUsize>,
    queued_len: Arc<AtomicUsize>,
    capacity: Option<usize>,
}

impl Collector for ChannelMemoryUsageTracker {
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        {
            let size_in_bytes = {
                let g: Gauge = Default::default();
                g.set(self.queued_size_in_bytes.load(Ordering::Relaxed) as _);
                g
            };
            let size_metric_name = format!("{}_{}_size", self.name, self.id);
            let size_metric_help = format!(
                "Qeueue message size of flume channel {}_{} in bytes",
                self.name, self.id
            );
            let size_metric_encoder = encoder.encode_descriptor(
                &size_metric_name,
                &size_metric_help,
                Some(&Unit::Bytes),
                size_in_bytes.metric_type(),
            )?;
            size_in_bytes.encode(size_metric_encoder)?;
        }
        {
            let size_in_bytes = {
                let g: Gauge = Default::default();
                g.set(self.total_size_in_bytes.load(Ordering::Relaxed) as _);
                g
            };
            let size_metric_name = format!("{}_{}_total_size", self.name, self.id);
            let size_metric_help = format!(
                "Total message size of flume channel {}_{} in bytes",
                self.name, self.id
            );
            let size_metric_encoder = encoder.encode_descriptor(
                &size_metric_name,
                &size_metric_help,
                Some(&Unit::Bytes),
                size_in_bytes.metric_type(),
            )?;
            size_in_bytes.encode(size_metric_encoder)?;
        }
        {
            let len_metric_name = format!("{}_{}_len", self.name, self.id);
            let len_metric_help = format!(
                "Queued message count of flume channel {}_{}",
                self.name, self.id
            );
            let len: Gauge = Default::default();
            len.set(self.queued_len.load(Ordering::Relaxed) as _);
            let len_metric_encoder = encoder.encode_descriptor(
                &len_metric_name,
                &len_metric_help,
                None,
                len.metric_type(),
            )?;
            len.encode(len_metric_encoder)?;
        }
        {
            let len_metric_name = format!("{}_{}_total_len", self.name, self.id);
            let len_metric_help = format!(
                "Total message count of flume channel {}_{}",
                self.name, self.id
            );
            let len: Gauge = Default::default();
            len.set(self.total_len.load(Ordering::Relaxed) as _);
            let len_metric_encoder = encoder.encode_descriptor(
                &len_metric_name,
                &len_metric_help,
                None,
                len.metric_type(),
            )?;
            len.encode(len_metric_encoder)?;
        }
        if let Some(capacity) = self.capacity {
            let cap_metric_name = format!("{}_{}_cap", self.name, self.id);
            let cap_metric_help = format!("Capacity of flume channel {}_{}", self.name, self.id);
            let cap: Gauge = Default::default();
            cap.set(capacity as _);
            let cap_metric_encoder = encoder.encode_descriptor(
                &cap_metric_name,
                &cap_metric_help,
                None,
                cap.metric_type(),
            )?;
            cap.encode(cap_metric_encoder)?;
        }

        Ok(())
    }
}

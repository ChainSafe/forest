// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

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

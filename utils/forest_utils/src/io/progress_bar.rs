// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{io::Stdout, str::FromStr, sync::Arc, time::Duration};

use parking_lot::{Mutex, RwLock};
pub use pbr::Units;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ProgressBarVisibility {
    Always,
    #[default]
    Auto,
    Never,
}

impl FromStr for ProgressBarVisibility {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(ProgressBarVisibility::Auto),
            "always" => Ok(ProgressBarVisibility::Always),
            "never" => Ok(ProgressBarVisibility::Never),
            _ => Err(Self::Err::msg(
                "Invalid progress bar visibility option. Must be one of [auto, always, never]",
            )),
        }
    }
}

static PROGRESS_BAR_VISIBILITY: RwLock<ProgressBarVisibility> =
    RwLock::new(ProgressBarVisibility::Auto);

/// Progress bar wrapper, allows suppressing progress bars.
#[derive(Clone)]
pub struct ProgressBar {
    inner: Arc<Mutex<pbr::ProgressBar<Stdout>>>,
    display: bool,
}

impl ProgressBar {
    pub fn new(size: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(pbr::ProgressBar::new(size))),
            display: Self::should_display(),
        }
    }

    pub fn message(&self, message: &str) {
        if self.display {
            self.inner.lock().message(message);
        }
    }

    pub fn set_max_refresh_rate(&self, w: Option<Duration>) {
        if self.display {
            self.inner.lock().set_max_refresh_rate(w);
        }
    }

    pub fn set_total(&self, i: u64) {
        if self.display {
            self.inner.lock().total = i;
        }
    }

    pub fn add(&self, i: u64) -> u64 {
        if self.display {
            self.inner.lock().add(i)
        } else {
            0
        }
    }

    pub fn inc(&self) -> u64 {
        if self.display {
            self.inner.lock().inc()
        } else {
            0
        }
    }

    pub fn set_units(&self, u: Units) {
        if self.display {
            self.inner.lock().set_units(u)
        }
    }

    pub fn set(&self, i: u64) -> u64 {
        if self.display {
            self.inner.lock().set(i)
        } else {
            0
        }
    }

    pub fn is_finish(&self) -> bool {
        self.inner.lock().is_finish
    }

    pub fn finish(&self) {
        if self.display {
            self.inner.lock().finish();
        }
    }

    pub fn finish_println(&self, s: &str) {
        if self.display {
            self.inner.lock().finish_println(s);
        }
    }

    /// Sets the visibility of progress bars (globally).
    pub fn set_progress_bars_visibility(visibility: ProgressBarVisibility) {
        *PROGRESS_BAR_VISIBILITY.write() = visibility;
    }

    /// Checks the global variable if progress bar should be shown.
    fn should_display() -> bool {
        match *PROGRESS_BAR_VISIBILITY.read() {
            ProgressBarVisibility::Always => true,
            ProgressBarVisibility::Auto => atty::is(atty::Stream::Stdout),
            ProgressBarVisibility::Never => false,
        }
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        self.finish()
    }
}

impl quickcheck::Arbitrary for ProgressBarVisibility {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        *g.choose(&[
            ProgressBarVisibility::Always,
            ProgressBarVisibility::Auto,
            ProgressBarVisibility::Never,
        ])
        .unwrap()
    }
}

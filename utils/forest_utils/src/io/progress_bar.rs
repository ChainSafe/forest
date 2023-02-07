// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{cell::RefCell, io::Stdout, str::FromStr, sync::RwLock, time::Duration};

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
pub struct ProgressBar {
    inner: RefCell<pbr::ProgressBar<Stdout>>,
    display: bool,
}

impl ProgressBar {
    pub fn new(size: u64) -> Self {
        Self {
            inner: RefCell::new(pbr::ProgressBar::new(size)),
            display: Self::should_display(),
        }
    }

    pub fn message(&self, message: &str) {
        if self.display {
            self.inner.borrow_mut().message(message);
        }
    }

    pub fn set_max_refresh_rate(&self, w: Option<Duration>) {
        if self.display {
            self.inner.borrow_mut().set_max_refresh_rate(w);
        }
    }

    pub fn add(&self, i: u64) -> u64 {
        if self.display {
            self.inner.borrow_mut().add(i)
        } else {
            0
        }
    }

    pub fn set_units(&self, u: Units) {
        if self.display {
            self.inner.borrow_mut().set_units(u)
        }
    }

    pub fn set(&self, i: u64) -> u64 {
        if self.display {
            self.inner.borrow_mut().set(i)
        } else {
            0
        }
    }

    pub fn finish(&self) {
        if self.display {
            self.inner.borrow_mut().finish();
        }
    }

    pub fn finish_println(&self, s: &str) {
        if self.display {
            self.inner.borrow_mut().finish_println(s);
        }
    }

    /// Sets the visibility of progress bars (globally).
    pub fn set_progress_bars_visibility(visibility: ProgressBarVisibility) {
        *PROGRESS_BAR_VISIBILITY
            .write()
            .expect("write must not fail") = visibility;
    }

    /// Checks the global variable if progress bar should be shown.
    fn should_display() -> bool {
        match *PROGRESS_BAR_VISIBILITY
            .read()
            .expect("read should not fail")
        {
            ProgressBarVisibility::Always => true,
            ProgressBarVisibility::Auto => atty::is(atty::Stream::Stdout),
            ProgressBarVisibility::Never => false,
        }
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

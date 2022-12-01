// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::str::FromStr;
use std::sync::RwLock;

use console::Term;
use indicatif::{ProgressBar as IndicatifProgressBar, ProgressDrawTarget, ProgressStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressBarVisibility {
    Always,
    Auto,
    Never,
}

impl Default for ProgressBarVisibility {
    fn default() -> Self {
        ProgressBarVisibility::Auto
    }
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
    inner: RefCell<IndicatifProgressBar>,
    display: bool,
    size: u64,
}

impl ProgressBar {
    pub fn new(size: u64) -> Self {
        Self {
            inner: RefCell::new(IndicatifProgressBar::with_draw_target(
                Some(size),
                ProgressDrawTarget::stdout(),
            )),
            display: Self::should_display(),
            size,
        }
    }

    pub fn message(&self, message: &str) {
        if self.display {
            self.inner.borrow_mut().set_message(message.to_string());
        }
    }

    pub fn set_max_refresh_rate_in_hz(&mut self, w: u8) {
        if self.display {
            self.inner = RefCell::new(IndicatifProgressBar::with_draw_target(
                Some(self.size),
                ProgressDrawTarget::term(Term::buffered_stdout(), w),
            ));
        }
    }

    pub fn add(&self, i: u64) -> u64 {
        if self.display {
            self.inner.borrow_mut().inc(i);
            self.inner.borrow_mut().position()
        } else {
            0
        }
    }

    pub fn set_bytes(&self) {
        if self.display {
            self.inner.borrow_mut().set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} {msg} {bytes}/{total_bytes} [{wide_bar:.cyan/blue}] {percent}% {bytes_per_sec} {eta}",
                )
                .unwrap()
                .progress_chars("=>-"),
            );
        }
    }

    pub fn set(&self, i: u64) -> u64 {
        if self.display {
            self.inner.borrow_mut().set_position(i);
            self.inner.borrow_mut().position()
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
            self.inner.borrow_mut().finish_with_message(s.to_string());
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

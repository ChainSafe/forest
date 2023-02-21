// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

pub const DEFAULT_RETRIES: i32 = 3;
pub const DEFAULT_DELAY: Duration = Duration::from_secs(60);

#[macro_export]
macro_rules! retry {
    ($func:ident, $max_retries:expr, $delay:expr $(, $arg:expr)*) => {{
        let mut retry_count = 0;
        loop {
            match $func($($arg),*).await {
                Ok(val) => break Ok(val),
                Err(err) => {
                    retry_count += 1;
                    info!("Retry attempt {} started with delay {:#?}.", retry_count, $delay);
                    if retry_count >= $max_retries {
                        info!("Maximum retries exceeded.");
                        break Err(err);
                    }
                    sleep($delay).await;
                }
            }
        }
    }};
}

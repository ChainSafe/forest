// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Retries a function call until `max_retries` is exceeded with a delay
// TODO(aatifsyed): this should be a function
#[macro_export]
macro_rules! retry {
    ($func:ident, $max_retries:expr, $delay:expr $(, $arg:expr)*) => {{
        let mut retry_count = 0;
        loop {
            match $func($($arg),*).await {
                Ok(val) => break Ok(val),
                Err(err) => {
                    retry_count += 1;
                    if retry_count >= $max_retries {
                        info!("Maximum retries exceeded.");
                        break Err(err);
                    }
                    log::warn!("{err:?}");
                    info!("Retry attempt {} started with delay {:#?}.", retry_count, $delay);
                    sleep($delay).await;
                }
            }
        }
    }};
}

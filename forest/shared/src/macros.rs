// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

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
                    info!("Retry attempt {} started with delay {:#?}.", retry_count, $delay);
                    sleep($delay).await;
                }
            }
        }
    }};
}

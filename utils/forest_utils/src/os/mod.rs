// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libc;
use std::io;

fn check_err<T: Ord + Default>(code: T) -> Result<T, anyhow::Error> {
    if code < T::default() {
        let e = io::Error::last_os_error();
        anyhow::bail!(e);
    }
    Ok(code)
}

/// Fetch the current resource limits or raise it to a new value if `raise_limit` is not `None`
pub fn fd_limit(raise_limit: Option<u64>) -> Result<u64, anyhow::Error> {
    let mut rlim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    unsafe {
        check_err(libc::getrlimit(libc::RLIMIT_NOFILE, &mut rlim))?;
    }
    match raise_limit {
        None => Ok(rlim.rlim_cur),
        Some(limit) => {
            rlim.rlim_cur = limit;
            unsafe {
                check_err(libc::setrlimit(libc::RLIMIT_NOFILE, &rlim))?;
            }
            Ok(limit)
        }
    }
}

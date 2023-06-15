// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod bundle;
pub mod daemon;

use raw_sync::events::{Event, EventInit as _};
use shared_memory::ShmemConf;

use lazy_static::lazy_static;
use tempfile::{Builder, TempPath};

lazy_static! {
    static ref IPC_PATH: TempPath = Builder::new()
        .prefix("forest-ipc")
        .tempfile()
        .expect("tempfile must succeed")
        .into_temp_path();
}

// The parent process and the daemonized child communicate through an Event in
// shared memory. The identity of the shared memory object is written to a
// temporary file. The parent process is responsible for cleaning up the file
// and the shared memory object.
pub fn ipc_shmem_conf() -> ShmemConf {
    ShmemConf::new()
        .size(Event::size_of(None))
        .force_create_flink()
        .flink(IPC_PATH.as_os_str())
}

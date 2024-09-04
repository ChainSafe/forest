// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub trait Clock<Tz: chrono::TimeZone> {
    fn now() -> chrono::DateTime<Tz>;
}

impl Clock<chrono::Utc> for chrono::Utc {
    fn now() -> chrono::DateTime<Self> {
        chrono::Utc::now()
    }
}

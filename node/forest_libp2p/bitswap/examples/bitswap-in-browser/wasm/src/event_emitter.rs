// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::*;

static mut EVENT_EMITTER: Option<EventEmitter> = None;

pub fn set_event_emitter(e: EventEmitter) {
    unsafe { EVENT_EMITTER = Some(e) };
}

pub fn get_event_emitter<'a>() -> Option<&'a EventEmitter> {
    unsafe { EVENT_EMITTER.as_ref() }
}

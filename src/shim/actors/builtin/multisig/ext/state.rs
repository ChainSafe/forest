// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl MultisigExt for State {
    fn get_vesting_schedule(&self) -> anyhow::Result<MsigVesting> {
        match self {
            State::V8(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
            State::V9(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
            State::V10(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
            State::V11(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
            State::V12(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
            State::V13(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
            State::V14(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
            State::V15(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
            State::V16(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
            State::V17(st) => Ok(MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            }),
        }
    }
}

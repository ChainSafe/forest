use fil_actors_runtime_v8::MessageAccumulator;
use fvm_shared::address::Protocol;

use crate::State;

pub struct StateSummary {
    pub entry_count: usize,
}

pub fn check_state_invariants(state: &State) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();

    state.entries.iter().enumerate().for_each(|(i, entry)| {
        acc.require(
            entry.receiver.protocol() == Protocol::ID,
            format!(
                "entry {i} receiver address {} must be ID protocol",
                entry.receiver
            ),
        );
        acc.require(
            entry.method_num > 0,
            format!("entry {i} has invalid method number {}", entry.method_num),
        );
    });

    (
        StateSummary {
            entry_count: state.entries.len(),
        },
        acc,
    )
}

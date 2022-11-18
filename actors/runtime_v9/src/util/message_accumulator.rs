use itertools::Itertools;
use std::{cell::RefCell, fmt::Display, rc::Rc};

use regex::Regex;

/// Accumulates a sequence of messages (e.g. validation failures).
#[derive(Debug, Default)]
pub struct MessageAccumulator {
    /// Accumulated messages.
    /// This is a `Rc<RefCell>` to support accumulators derived from `with_prefix()` accumulating to
    /// the same underlying collection.
    msgs: Rc<RefCell<Vec<String>>>,
    /// Optional prefix to all new messages, e.g. describing higher level context.
    prefix: String,
}

impl MessageAccumulator {
    /// Returns a new accumulator backed by the same collection, that will prefix each new message with
    /// a formatted string.
    pub fn with_prefix<S: AsRef<str>>(&self, prefix: S) -> Self {
        MessageAccumulator {
            msgs: self.msgs.clone(),
            prefix: self.prefix.to_owned() + prefix.as_ref(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.msgs.borrow().is_empty()
    }

    pub fn messages(&self) -> Vec<String> {
        self.msgs.borrow().to_owned()
    }

    /// Returns the number of accumulated messages
    pub fn len(&self) -> usize {
        self.msgs.borrow().len()
    }

    /// Adds a message to the accumulator
    pub fn add<S: AsRef<str>>(&self, msg: S) {
        self.msgs
            .borrow_mut()
            .push(format!("{}{}", self.prefix, msg.as_ref()));
    }

    /// Adds messages from another accumulator to this one
    pub fn add_all(&self, other: &Self) {
        self.msgs
            .borrow_mut()
            .extend_from_slice(&other.msgs.borrow());
    }

    /// Adds a message if predicate is false
    pub fn require<S: AsRef<str>>(&self, predicate: bool, msg: S) {
        if !predicate {
            self.add(msg);
        }
    }

    /// Adds a message if result is `Err`. Underlying error must be `Display`.
    pub fn require_no_error<V, E: Display, S: AsRef<str>>(&self, result: Result<V, E>, msg: S) {
        if let Err(e) = result {
            self.add(format!("{}: {e}", msg.as_ref()));
        }
    }

    /// Panic if the accumulator isn't empty. The acculumated messages are included in the panic message.
    #[track_caller]
    pub fn assert_empty(&self) {
        assert!(self.is_empty(), "{}", self.messages().join("\n"))
    }

    /// Asserts the accumulator contains messages matching provided pattern *in the given order*.
    #[track_caller]
    pub fn assert_expected(&self, expected_patterns: &[Regex]) {
        let messages = self.messages();
        assert!(
            messages.len() == expected_patterns.len(),
            "Incorrect number of accumulator messages. Actual: {}.\nExpected: {}",
            messages.join("\n"),
            expected_patterns
                .iter()
                .map(|regex| regex.as_str())
                .join("\n")
        );

        messages
            .iter()
            .zip(expected_patterns)
            .for_each(|(message, pattern)| {
                assert!(
                    pattern.is_match(message),
                    "message does not match. Actual: {}, expected: {}",
                    message,
                    pattern.as_str()
                );
            });
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn adds_messages() {
        let acc = MessageAccumulator::default();
        acc.add("Cthulhu");
        assert_eq!(acc.len(), 1);

        let msgs = acc.messages();
        assert_eq!(msgs, vec!["Cthulhu"]);

        acc.add("Azathoth");
        assert_eq!(acc.len(), 2);

        let msgs = acc.messages();
        assert_eq!(msgs, vec!["Cthulhu", "Azathoth"]);
    }

    #[test]
    fn adds_on_predicate() {
        let acc = MessageAccumulator::default();
        acc.require(true, "Cthulhu");

        assert_eq!(acc.len(), 0);
        assert!(acc.is_empty());

        acc.require(false, "Azathoth");
        let msgs = acc.messages();
        assert_eq!(acc.len(), 1);
        assert_eq!(msgs, vec!["Azathoth"]);
        assert!(!acc.is_empty());
    }

    #[test]
    fn require_no_error() {
        let fiasco: Result<(), String> = Err("fiasco".to_owned());
        let acc = MessageAccumulator::default();
        acc.require_no_error(fiasco, "Cthulhu says");

        let msgs = acc.messages();
        assert_eq!(acc.len(), 1);
        assert_eq!(msgs, vec!["Cthulhu says: fiasco"]);
    }

    #[test]
    fn prefixes() {
        let acc = MessageAccumulator::default();
        acc.add("peasant");

        let gods_acc = acc.with_prefix("elder god -> ");
        gods_acc.add("Cthulhu");

        assert_eq!(acc.messages(), vec!["peasant", "elder god -> Cthulhu"]);
        assert_eq!(gods_acc.messages(), vec!["peasant", "elder god -> Cthulhu"]);
    }

    #[test]
    fn add_all() {
        let acc1 = MessageAccumulator::default();
        acc1.add("Cthulhu");

        let acc2 = MessageAccumulator::default();
        acc2.add("Azathoth");

        let acc3 = MessageAccumulator::default();
        acc3.add_all(&acc1);
        acc3.add_all(&acc2);

        assert_eq!(2, acc3.len());
        assert_eq!(acc3.messages(), vec!["Cthulhu", "Azathoth"]);
    }
}

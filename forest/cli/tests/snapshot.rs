// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! Basic UI tests for the snapshot subcommand
//! We only cover the `testable` aspects, so no fetching snapshots (and no validating snapshots)
//!
//! See `forest_cli_check.sh` for those tests

use std::fmt::Display;

use assert_cmd::{assert::Assert, Command};
use predicates::boolean::PredicateBooleanExt as _;
use tap::Pipe as _;

/// Just check that the dir command produces some output
#[test]
fn dir() {
    forest_cli()
        .args(["snapshot", "default-location"])
        .pipe_ref_mut(run_and_log("snapshot default-location"))
        .success()
        .stdout(predicates::str::is_empty().not());
}

fn forest_cli() -> Command {
    Command::cargo_bin("forest-cli").expect("couldn't find test binary")
}

/// Run the command, logging its output and exit status to stdout, and returning an assert handle
/// This is useful for sequences of commands
fn run_and_log(message: impl Display) -> impl FnMut(&mut Command) -> Assert {
    move |command| {
        println!(">>>>{message}>>>>");
        let assert = command.assert();
        let output = assert.get_output();
        println!("===exit===");
        println!("{:?}", output.status);
        println!("===stdout===");
        match std::str::from_utf8(&output.stdout) {
            Ok(s) => println!("{s}"),
            Err(_) => println!("<binary>"),
        }
        println!("===stderr===");
        match std::str::from_utf8(&output.stderr) {
            Ok(s) => println!("{s}"),
            Err(_) => println!("<binary>"),
        }
        println!("<<<<{message}<<<<");
        assert
    }
}

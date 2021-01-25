#![cfg(feature = "integration_tests")]

use anyhow::{Error, Result};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

const CONFIGS_PATH: &str = "./configs";

#[test]
pub fn single_producer_multiple_consumer() -> Result<(), Error> {
  let forest_sp1_config = format!("{}/forest/single_producer_1.toml", CONFIGS_PATH);

  let mut output = Command::new("bash")
    .arg("-C")
    .arg(format!(
      "../../target/release/forest --config {}",
      forest_sp1_config
    ))
    .stdout(Stdio::piped())
    .spawn()
    .expect("forest failed to start");

  let mut stdout = BufReader::new(output.stdout.as_mut().unwrap());
  let mut line = String::new();

  let mut reading = true;

  while reading {
    stdout.read_line(&mut line)?;

    if line.contains("forest::daemon") {
      assert_eq!(line, "some stdout assertion");

      reading = false;
    }
  }

  Ok(())
}

#[test]
pub fn interop_bootstrap_sync() {
  unimplemented!();
}

#[test]
pub fn interop_forest_producer() {
  unimplemented!();
}

#[test]
pub fn interop_lotus_consumer() {
  unimplemented!();
}

#[test]
pub fn interop_lotus_produce() {
  unimplemented!();
}

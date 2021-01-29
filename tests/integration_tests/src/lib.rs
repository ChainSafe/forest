#![cfg(test)]
use anyhow::{Error, Result};
use async_std::process::{Command, Stdio};
use async_std::sync::{Arc, Mutex};
use std::io::{BufRead, BufReader};

struct Process {
  output: Arc<Mutex<String>>,
}

impl Process {
  pub fn new() -> Self {
    let output = Arc::new(Mutex::new(String::new()));

    Self { output }
  }
}

#[async_std::test]
pub async fn single_producer_multiple_consumer() -> Result<(), Error> {
  let mut output = Command::new("bash")
    .arg("-C")
    .arg(format!("forest --config {}", forest_sp1_config))
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

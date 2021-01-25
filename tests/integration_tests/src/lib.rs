#![cfg(feature = "submodule_tests")]

use cmd_lib::run_cmd;

const CONFIGS_PATH: &str = "tests/integration_tests/configs";

#[test]
pub fn single_producer_multiple_consumer() {
  let forest_sp1_config = format!("{}/forest/single_producer_1.toml", CONFIGS_PATH);
  let result = run_cmd!(|forest_sp1_config| forest --config "#forest_sp1_config")?;

  assert_eq!(result, "some stdout assertion");
}

#[test]
pub fn interop_bootstrap_sync() {
  !unimplemented
}

#[test]
pub fn interop_forest_producer() {
  !unimplemented
}

#[test]
pub fn interop_lotus_consumer() {
  !unimplemented
}

#[test]
pub fn interop_lotus_produce() {
  !unimplemented
}

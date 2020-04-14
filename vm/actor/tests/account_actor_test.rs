use actor::{SYSTEM_ACTOR_ADDR};
// use crate::tests::mock_rt::*;
use db::MemoryDB;
use address::Address;

#[path = "mock_rt.rs"]
mod mock_rt;
use mock_rt::*;

#[test]
fn no() {
    let bs = MemoryDB::default();
    let rt = MockRuntime::new(&bs, Address::default());
    rt.expect_validate_caller_addr(&vec![SYSTEM_ACTOR_ADDR.clone()])
    
}

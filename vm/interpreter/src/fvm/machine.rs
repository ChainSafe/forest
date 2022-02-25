// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::fvm::externs::ForestExterns;
use fvm::machine::{Machine, MachineContext};
use fvm::state_tree::ActorState;
use fvm::Config;
use fvm_shared::ActorID;
use ipld_blockstore::BlockStore;
use ipld_blockstore::FvmStore;
use vm::TokenAmount;

pub struct ForestMachine<C, DB: 'static> {
    pub machine: fvm::machine::DefaultMachine<FvmStore<DB>, ForestExterns>,
    pub circ_supply: C,
}

impl<C: 'static, DB: BlockStore> Machine for ForestMachine<C, DB> {
    type Blockstore =
        <fvm::machine::DefaultMachine<FvmStore<DB>, ForestExterns> as Machine>::Blockstore;
    type Externs = ForestExterns;

    fn engine(&self) -> &wasmtime::Engine {
        self.machine.engine()
    }

    fn config(&self) -> &Config {
        self.machine.config()
    }

    fn blockstore(&self) -> &Self::Blockstore {
        self.machine.blockstore()
    }

    fn context(&self) -> &MachineContext {
        self.machine.context()
    }

    fn externs(&self) -> &Self::Externs {
        self.machine.externs()
    }

    fn state_tree(&self) -> &fvm::state_tree::StateTree<Self::Blockstore> {
        self.machine.state_tree()
    }

    fn state_tree_mut(&mut self) -> &mut fvm::state_tree::StateTree<Self::Blockstore> {
        self.machine.state_tree_mut()
    }

    fn create_actor(
        &mut self,
        addr: &fvm_shared::address::Address,
        act: ActorState,
    ) -> fvm::kernel::Result<ActorID> {
        self.machine.create_actor(addr, act)
    }

    fn load_module(&self, code: &cid_orig::Cid) -> fvm::kernel::Result<wasmtime::Module> {
        self.machine.load_module(code)
    }

    fn transfer(
        &mut self,
        from: ActorID,
        to: ActorID,
        value: &TokenAmount,
    ) -> fvm::kernel::Result<()> {
        self.machine.transfer(from, to, value)
    }

    fn consume(self) -> Self::Blockstore {
        self.machine.consume()
    }

    fn flush(&mut self) -> fvm::kernel::Result<cid_orig::Cid> {
        self.machine.flush()
    }
}

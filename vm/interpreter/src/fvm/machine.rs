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

pub struct ForestMachine<DB: 'static> {
    pub machine: fvm::machine::DefaultMachine<FvmStore<DB>, ForestExterns>,
    pub circ_supply: Option<TokenAmount>,
}

impl<DB: BlockStore> Machine for ForestMachine<DB> {
    type Blockstore =
        <fvm::machine::DefaultMachine<FvmStore<DB>, ForestExterns> as Machine>::Blockstore;
    type Externs = ForestExterns;

    fn engine(&self) -> &fvm::machine::Engine {
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

    fn builtin_actors(&self) -> &fvm_shared::actor::builtin::Manifest {
        self.machine.builtin_actors()
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

    // fn load_module(&self, code: &cid_orig::Cid) -> fvm::kernel::Result<wasmtime::Module> {
    //     self.machine.load_module(code)
    // }

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

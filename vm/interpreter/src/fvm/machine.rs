// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::fvm::externs::ForestExterns;
use cid::Cid;
use forest_ipld_blockstore::BlockStore;
use forest_vm::TokenAmount;
use fvm::machine::{Machine, MachineContext};
use fvm::state_tree::ActorState;
use fvm_shared::ActorID;

pub struct ForestMachine<DB: 'static> {
    pub machine: fvm::machine::DefaultMachine<DB, ForestExterns<DB>>,
    pub circ_supply: Option<TokenAmount>,
}

impl<DB: BlockStore> Machine for ForestMachine<DB> {
    type Blockstore = <fvm::machine::DefaultMachine<DB, ForestExterns<DB>> as Machine>::Blockstore;
    type Externs = ForestExterns<DB>;

    fn engine(&self) -> &fvm::machine::Engine {
        self.machine.engine()
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

    fn transfer(
        &mut self,
        from: ActorID,
        to: ActorID,
        value: &TokenAmount,
    ) -> fvm::kernel::Result<()> {
        self.machine.transfer(from, to, value)
    }

    fn flush(&mut self) -> fvm::kernel::Result<Cid> {
        self.machine.flush()
    }

    fn into_store(self) -> Self::Blockstore {
        self.machine.into_store()
    }
}

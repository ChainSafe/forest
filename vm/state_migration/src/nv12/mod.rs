// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod miner;

pub use miner::miner_migrator_v4;

use crate::nil_migrator;
use crate::StateMigration;
use actor_interface::{actorv3, actorv4};
use ipld_blockstore::BlockStore;

impl<BS: BlockStore + Send + Sync> StateMigration<BS> {
    // Initializes the migrations map with Nil migrators for network version 12 upgrade
    pub fn set_nil_migrations(&mut self) {
        self.migrations.insert(
            *actorv3::ACCOUNT_ACTOR_CODE_ID,
            nil_migrator(*actorv4::ACCOUNT_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv3::CRON_ACTOR_CODE_ID,
            nil_migrator(*actorv4::CRON_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv3::INIT_ACTOR_CODE_ID,
            nil_migrator(*actorv4::INIT_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv3::MULTISIG_ACTOR_CODE_ID,
            nil_migrator(*actorv4::MULTISIG_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv3::PAYCH_ACTOR_CODE_ID,
            nil_migrator(*actorv4::PAYCH_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv3::REWARD_ACTOR_CODE_ID,
            nil_migrator(*actorv4::REWARD_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv3::MARKET_ACTOR_CODE_ID,
            nil_migrator(*actorv4::MARKET_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv3::POWER_ACTOR_CODE_ID,
            nil_migrator(*actorv4::POWER_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv3::SYSTEM_ACTOR_CODE_ID,
            nil_migrator(*actorv4::SYSTEM_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv3::VERIFREG_ACTOR_CODE_ID,
            nil_migrator(*actorv4::VERIFREG_ACTOR_CODE_ID),
        );
    }
}

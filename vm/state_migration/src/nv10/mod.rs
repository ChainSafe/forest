// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod util;
pub mod init;
pub mod multisig;
pub mod paych;
pub mod verifreg;
pub mod power;
pub mod miner;
pub mod market;

pub use miner::miner_migrator_v3;
pub use init::init_migrator_v3;
pub use market::market_migrator_v3;
pub use multisig::multisig_migrator_v3;
pub use paych::paych_migrator_v3;
pub use power::power_migrator_v3;
pub use verifreg::verifreg_migrator_v3;

use crate::StateMigration;
use ipld_blockstore::BlockStore;
use crate::nil_migrator;
use actor_interface::{actorv2, actorv3};

impl<BS: BlockStore + Send + Sync> StateMigration<BS> {
    // Initializes the migrations map with Nil migrators for network version 10 upgrade
    pub fn set_nil_migrations_v3(&mut self) {
        self.migrations.insert(
            *actorv2::ACCOUNT_ACTOR_CODE_ID,
            nil_migrator(*actorv3::ACCOUNT_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv2::CRON_ACTOR_CODE_ID,
            nil_migrator(*actorv3::CRON_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv2::INIT_ACTOR_CODE_ID,
            nil_migrator(*actorv3::INIT_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv2::MULTISIG_ACTOR_CODE_ID,
            nil_migrator(*actorv3::MULTISIG_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv2::PAYCH_ACTOR_CODE_ID,
            nil_migrator(*actorv3::PAYCH_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv2::REWARD_ACTOR_CODE_ID,
            nil_migrator(*actorv3::REWARD_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv2::MARKET_ACTOR_CODE_ID,
            nil_migrator(*actorv3::MARKET_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv2::POWER_ACTOR_CODE_ID,
            nil_migrator(*actorv3::POWER_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv2::SYSTEM_ACTOR_CODE_ID,
            nil_migrator(*actorv3::SYSTEM_ACTOR_CODE_ID),
        );
        self.migrations.insert(
            *actorv2::VERIFREG_ACTOR_CODE_ID,
            nil_migrator(*actorv3::VERIFREG_ACTOR_CODE_ID),
        );
    }
}

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod eam;
mod init;
pub mod migration;
mod system;
pub mod verifier;

use fvm_ipld_blockstore::Blockstore;

use crate::{nil_migrator, StateMigration};

macro_rules! make_actors {
    ($($name:ident, $value: expr),+) => {
        use cid::Cid;
        use lazy_static::lazy_static;
        lazy_static! {
            $(
            pub static ref $name: Cid = Cid::try_from($value).unwrap();
            )+
        }
    };
}

pub mod calibnet {
    pub mod v9 {
        make_actors!(
            ACCOUNT,
            "bafk2bzaceavfgpiw6whqigmskk74z4blm22nwjfnzxb4unlqz2e4wg3c5ujpw",
            CRON,
            "bafk2bzaceb7hxmudhvkizszbmmf2ur2qfnfxfkok3xmbrlifylx6huw4bb3s4",
            DATACAP,
            "bafk2bzaceanmwcfjfj65xy275rrfqqgoblnuqirdg6zwhc6qhbfhpphomvceu",
            INIT,
            "bafk2bzaceczqxpivlxifdo5ohr2rx5ny4uyvssm6tkf7am357xm47x472yxu2",
            MULTISIG,
            "bafk2bzacec6gmi7ucukr3bk67akaxwngohw3lsg3obvdazhmfhdzflkszk3tg",
            PAYMENT_CHANNEL,
            "bafk2bzacec4kg3bfjtssvv2b4wizlbdk3pdtrg5aknzgeb3a6rmksgurpynca",
            REWARD,
            "bafk2bzacebpptqhcw6mcwdj576dgpryapdd2zfexxvqzlh3aoc24mabwgmcss",
            STORAGE_MARKET,
            "bafk2bzacebkfcnc27d3agm2bhzzbvvtbqahmvy2b2nf5xyj4aoxehow3bules",
            STORAGE_MINER,
            "bafk2bzacebz4na3nq4gmumghegtkaofrv4nffiihd7sxntrryfneusqkuqodm",
            STORAGE_POWER,
            "bafk2bzaceburxajojmywawjudovqvigmos4dlu4ifdikogumhso2ca2ccaleo",
            SYSTEM,
            "bafk2bzaceaue3nzucbom3tcclgyaahy3iwvbqejsxrohiquakvvsjgbw3shac",
            VERIFIED_REGISTRY,
            "bafk2bzacebh7dj6j7yi5vadh7lgqjtq42qi2uq4n6zy2g5vjeathacwn2tscu"
        );
    }

    pub mod v10 {
        make_actors!(
            ACCOUNT,
            "bafk2bzacebhfuz3sv7duvk653544xsxhdn4lsmy7ol7k6gdgancyctvmd7lnq",
            CRON,
            "bafk2bzacecw2yjb6ysieffa7lk7xd32b3n4ssowvafolt7eq52lp6lk4lkhji",
            DATACAP,
            "bafk2bzaceaot6tv6p4cat3cg5fknq22htosw3p5rwyijmdsraatwqyc4qyero",
            EAM,
            "bafk2bzacec5untyj6cefdsfm47wckozw6wt6svqqh5dzh63nu4f6dvf26fkco",
            ETH_ACCOUNT,
            "bafk2bzacebiyrhz32xwxi6xql67aaq5nrzeelzas472kuwjqmdmgwotpkj35e",
            EVM,
            "bafk2bzaceblpgzid4qjfavuiht6uwvq2lznshklk2qmf5akm3dzx2fczdqdxc",
            INIT,
            "bafk2bzacedhxbcglnonzruxf2jpczara73eh735wf2kznatx2u4gsuhgqwffq",
            MULTISIG,
            "bafk2bzacebv5gdlte2pyovmz6s37me6x2rixaa6a33w6lgqdohmycl23snvwm",
            PAYMENT_CHANNEL,
            "bafk2bzacea7ngq44gedftjlar3j3ql3dmd7e7xkkb6squgxinfncybfmppmlc",
            PLACEHOLDER,
            "bafk2bzacedfvut2myeleyq67fljcrw4kkmn5pb5dpyozovj7jpoez5irnc3ro",
            REWARD,
            "bafk2bzacea3yo22x4dsh4axioshrdp42eoeugef3tqtmtwz5untyvth7uc73o",
            STORAGE_MARKET,
            "bafk2bzacecclsfboql3iraf3e66pzuh3h7qp3vgmfurqz26qh5g5nrexjgknc",
            STORAGE_MINER,
            "bafk2bzacedu4chbl36rilas45py4vhqtuj6o7aa5stlvnwef3kshgwcsmha6y",
            STORAGE_POWER,
            "bafk2bzacedu3c67spbf2dmwo77ymkjel6i2o5gpzyksgu2iuwu2xvcnxgfdjg",
            SYSTEM,
            "bafk2bzacea4mtukm5zazygkdbgdf26cpnwwif5n2no7s6tknpxlwy6fpq3mug",
            VERIFIED_REGISTRY,
            "bafk2bzacec67wuchq64k7kgrujguukjvdlsl24pgighqdx5vgjhyk6bycrwnc",
            _MANIFEST,
            "bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo"
        );
    }
}

impl<BS: Blockstore + Clone + Send + Sync> StateMigration<BS> {
    // Initializes the migrations map with Nil migrators for network version 18
    // upgrade
    pub fn add_nil_migrations(&mut self) {
        let nil_migrations = [
            (*calibnet::v9::ACCOUNT, *calibnet::v10::ACCOUNT),
            (*calibnet::v9::CRON, *calibnet::v10::CRON),
            (*calibnet::v9::DATACAP, *calibnet::v10::DATACAP),
            (*calibnet::v9::MULTISIG, *calibnet::v10::MULTISIG),
            (
                *calibnet::v9::PAYMENT_CHANNEL,
                *calibnet::v10::PAYMENT_CHANNEL,
            ),
            (*calibnet::v9::REWARD, *calibnet::v10::REWARD),
            (
                *calibnet::v9::STORAGE_MARKET,
                *calibnet::v10::STORAGE_MARKET,
            ),
            (*calibnet::v9::STORAGE_POWER, *calibnet::v10::STORAGE_POWER),
            (*calibnet::v9::STORAGE_MINER, *calibnet::v10::STORAGE_MINER),
            (
                *calibnet::v9::VERIFIED_REGISTRY,
                *calibnet::v10::VERIFIED_REGISTRY,
            ),
        ];

        self.migrations.extend(
            nil_migrations
                .into_iter()
                .map(|(from, to)| (from, nil_migrator(to))),
        );
    }

    pub fn add_nv_18_migrations(&mut self) {
        self.add_migrator(
            *calibnet::v9::INIT,
            init::init_migrator(*calibnet::v10::INIT),
        );

        self.add_migrator(
            *calibnet::v9::SYSTEM,
            system::system_migrator(*calibnet::v10::SYSTEM),
        )
    }
}

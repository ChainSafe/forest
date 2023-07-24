// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::beacon::{mock_beacon::MockBeacon, BeaconEntry, BeaconPoint, BeaconSchedule};
use crate::shim::clock::ChainEpoch;
use crate::shim::{address::Address, version::NetworkVersion};
use fvm_ipld_encoding::{from_slice, to_vec};

use crate::blocks::{errors::Error, BlockHeader};

impl quickcheck::Arbitrary for BlockHeader {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        // XXX: More fields can be randomly generated.
        let block_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .epoch(ChainEpoch::arbitrary(g))
            .build()
            .unwrap();
        block_header
    }
}

#[test]
fn symmetric_header_encoding() {
    // This test vector is pulled from space race, and contains a valid signature
    let bz = hex::decode("904300e8078158608798de4e49e02ee129920224ea767650aa6e693857431cc95b5a092a57d80ef4d841ebedbf09f7680a5e286cd297f40100b496648e1fa0fd55f899a45d51404a339564e7d4809741ba41d9fcc8ac0261bf521cd5f718389e81354eff2aa52b338201586084d8929eeedc654d6bec8bb750fcc8a1ebf2775d8167d3418825d9e989905a8b7656d906d23dc83e0dad6e7f7a193df70a82d37da0565ce69b776d995eefd50354c85ec896a2173a5efed53a27275e001ad72a3317b2190b98cceb0f01c46b7b81821a00013cbe5860ae1102b76dea635b2f07b7d06e1671d695c4011a73dc33cace159509eac7edc305fa74495505f0cd0046ee0d3b17fabc0fc0560d44d296c6d91bcc94df76266a8e9d5312c617ca72a2e186cadee560477f6d120f6614e21fb07c2390a166a25981820358c0b965705cec77b46200af8fb2e47c0eca175564075061132949f00473dcbe74529c623eb510081e8b8bd34418d21c646485d893f040dcfb7a7e7af9ae4ed7bd06772c24fb0cc5b8915300ab5904fbd90269d523018fbf074620fd3060d55dd6c6057b4195950ac4155a735e8fec79767f659c30ea6ccf0813a4ab2b4e60f36c04c71fb6c58efc123f60c6ea8797ab3706a80a4ccc1c249989934a391803789ab7d04f514ee0401d0f87a1f5262399c451dcf5f7ec3bb307fc6f1a41f5ff3a5ddb81d82a5827000171a0e402209a0640d0620af5d1c458effce4cbb8969779c9072b164d3fe6f5179d6378d8cd4300310001d82a5827000171a0e402208fbc07f7587e2efebab9ff1ab27c928881abf9d1b7e5ad5206781415615867aed82a5827000171a0e40220e5658b3d18cd06e1db9015b4b0ec55c123a24d5be1ea24d83938c5b8397b4f2fd82a5827000171a0e402209967f10c4c0e336b3517d3a972f701dadea5b41ce33defb126b88e650cf884545861028ec8b64e2d93272f97edcab1f56bcad4a2b145ea88c232bfae228e4adbbd807e6a41740cc8cb569197dae6b2cbf8c1a4035e81fd7805ccbe88a5ec476bcfa438db4bd677de06b45e94310533513e9d17c635940ba8fa2650cdb34d445724c5971a5f44387e5861028a45c70a39fe8e526cbb6ba2a850e9063460873d6329f26cc2fc91972256c40249dba289830cc99619109c18e695d78012f760e7fda1b68bc3f1fe20ff8a017044753da38ca6384de652f3ee13aae5b64e6f88f85fd50d5c862fed3c1f594ace004500053724e0").unwrap();
    let header = from_slice::<BlockHeader>(&bz).unwrap();
    assert_eq!(to_vec(&header).unwrap(), bz);

    // Verify the signature of this block header using the resolved address used to
    // sign. This is a valid signature, but if the block header vector
    // changes, the address should need to as well.
    header
            .check_block_signature(
                &"f3vfs6f7tagrcpnwv65wq3leznbajqyg77bmijrpvoyjv3zjyi3urq25vigfbs3ob6ug5xdihajumtgsxnz2pa"
                .parse()
                .unwrap())
            .unwrap();
}

#[test]
fn beacon_entry_exists() {
    // Setup
    let block_header = BlockHeader::builder()
        .miner_address(Address::new_id(0))
        .beacon_entries(Vec::new())
        .build()
        .unwrap();
    let beacon_schedule = BeaconSchedule(vec![BeaconPoint {
        height: 0,
        beacon: MockBeacon::default(),
    }]);
    let chain_epoch = 0;
    let beacon_entry = BeaconEntry::new(1, vec![]);
    // Validate_block_drand
    if let Err(e) = block_header.validate_block_drand(
        NetworkVersion::V16,
        &beacon_schedule,
        chain_epoch,
        &beacon_entry,
    ) {
        // Assert error is for not including a beacon entry in the block
        match e {
            Error::Validation(why) => {
                assert_eq!(why, "Block must include at least 1 beacon entry");
            }
            _ => {
                panic!("validate block drand must detect a beacon entry in the block header");
            }
        }
    }
}

// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{CachingBlockHeader, TipsetKey};
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use cid::Cid;

fn create_test_header(
    miner: Address,
    epoch: ChainEpoch,
    parents: TipsetKey,
    timestamp: Option<u64>,
) -> CachingBlockHeader {
    use crate::blocks::RawBlockHeader;
    use crate::shim::econ::TokenAmount;
    use num::BigInt;

    let raw_header = RawBlockHeader {
        miner_address: miner,
        epoch,
        parents,
        weight: BigInt::from(0),
        state_root: Cid::default(),
        message_receipts: Cid::default(),
        messages: Cid::default(),
        timestamp: timestamp.unwrap_or(0),
        parent_base_fee: TokenAmount::from_atto(0),
        ticket: None,
        election_proof: None,
        beacon_entries: Vec::new(),
        winning_post_proof: Vec::new(),
        bls_aggregate: None,
        signature: None,
        fork_signal: 0,
    };

    CachingBlockHeader::new(raw_header)
}

#[test]
fn test_filter_double_fork_mining() {
    use crate::slasher::db::SlasherDb;
    use crate::utils::encoding::from_slice_with_fallback;

    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("slasher_test_double_fork"));
    let mut db = SlasherDb::new(std::env::temp_dir().join("slasher_test_double_fork"))
        .expect("Failed to create slasher db");

    let bz = hex::decode("90440093f37a815860afc066776903f91344937c1ac1351070e68813aad3623184211bc5f1aaa81eba16d6e48550800bb9bf4195d9fbd5651c1045dfa1e71b0cd8e17642f3469aa5adf2e69b18b7f74031324d1826d0f84ccd01114f8993c9cf8ec366f60ae7a78dea82015860a5a10dbc3be5ddbc50b998d9276cc2a38f8684e349318eae898ee5c30e081625824749e08d94dba48c38ed68c0725852164a81fcfaee16d36270099962e49e2cc878306962dd16050713477873f8e66410f3bde90d878c503e5f637dc4583f5181821a0133c4865830aed926ff74d16217e4c8dcc6d5682aaf4adb43311be06f3022645bbde9a5499c6ee413f8f3a489a923967e20f5a5a4c481820358c098e17ca5d50ff3ef14dac93283bee5d16d0ed50b82470acebab491ead4bc49a1f05a39308f0c862e8aae5d1466bb88d299f0e3901eba20e9848963499ff04e8295045b35da1cc79c0bd45d610f6dfa4bc78d1a9ec177c6fca42dc6a140cca32a043339634f7a29cd3f13fb8d145e7d068356b1c5b9fe49bc32c878a67f38a326ba0d8d55d55fbd3ee5fe298ae78487d3a328380d68c4a94c82361a63b89f7d75e3ecc6b7b19db03915e8df0ae8dc293f1cf3134d821d6fb64792573d0e7988dc84d82a5827000171a0e40220493e9e471b6c3ff1b7a2b1c5037ac0c73f71a0459034816ec4e487b4a9da53d8d82a5827000171a0e402209d477b2ad612a1b1da4b4958a6a2b3b30a39dac8abed837ec640d55a62afce54d82a5827000171a0e402204b3763feb0a4c3810b5570a35adc972f5ed657924d88b08c5e980553af4451bdd82a5827000171a0e4022004ce083decc01e9c4fb879574544e1c76116e65ff71fd3451d663ebf83807dab46001cba67cad41a004ed726d82a5827000171a0e40220b6b5e4010c9e5adcd72c44a7a799359893cf20abd27ee3d2ba845d08132f998cd82a5827000171a0e40220cecdb58a95df93eae8e28bc14d1bc832a0528db8ab593fa996560add1d511430d82a5827000171a0e40220c24231c099a8d8dc5d829060a64e28b38e5aae09527ba54365b3f5ae7cc07d5b586102828268aa61490e93d026433a3502f9505c1c290f558ef9a318b85b10a3b1dd10d8f2dc083c67feb50556b0cb201d4e49162a10bc98c8403ca96ed95de5317441182f657c211e6652fc3d06e596728888df25ec6aa830ac01b4151b83f60102eb1a68816ed4586102b310a8f6ff08b58d12435859a184d78f0f3f909eaaf68edb4c464cfcbb151fcab597286946a4114d01e53092d5e3bacd0868358adaf23af10b21307a0f3017528cc84e496bb59182fd0910f12bef73ecb725f68feaa246a4e537dde40c74acf500420064").unwrap();
    let bh_1 = from_slice_with_fallback::<CachingBlockHeader>(&bz).unwrap();

    let bz = hex::decode("90440093f37a815860afc066776903f91344937c1ac1351070e68813aad3623184211bc5f1aaa81eba16d6e48550800bb9bf4195d9fbd5651c1045dfa1e71b0cd8e17642f3469aa5adf2e69b18b7f74031324d1826d0f84ccd01114f8993c9cf8ec366f60ae7a78dea82015860a5a10dbc3be5ddbc50b998d9276cc2a38f8684e349318eae898ee5c30e081625824749e08d94dba48c38ed68c0725852164a81fcfaee16d36270099962e49e2cc878306962dd16050713477873f8e66410f3bde90d878c503e5f637dc4583f5181821a0133c4865830aed926ff74d16217e4c8dcc6d5682aaf4adb43311be06f3022645bbde9a5499c6ee413f8f3a489a923967e20f5a5a4c481820358c0b003f7619deaedafc85b00b2c01a1a2617ab0cf91248b9d1a1ace8d37a94dd9ada9d9830558620b96ea648f0d8818b6fb45f97134bc2c946279a09a257788701ed6aca5fe7e0465836dd26c0ebfe6b50984ac38cfb5f7d7ce3c73d69f5600cc40ba1b1f4d25f0b172685b9361040894460a9ffc2448dfbc234cb5f27ca4e26cba630bd27ff80766cb85d675db1c3ec67b0c1a5157b72bf7652a8ae9758f78e211811149d1bc39a0a905e394d62da01036e18e1ac921b995f09e85957ed7464ae84d82a5827000171a0e40220493e9e471b6c3ff1b7a2b1c5037ac0c73f71a0459034816ec4e487b4a9da53d8d82a5827000171a0e402209d477b2ad612a1b1da4b4958a6a2b3b30a39dac8abed837ec640d55a62afce54d82a5827000171a0e402204b3763feb0a4c3810b5570a35adc972f5ed657924d88b08c5e980553af4451bdd82a5827000171a0e4022004ce083decc01e9c4fb879574544e1c76116e65ff71fd3451d663ebf83807dab46001cba67cad41a004ed726d82a5827000171a0e40220b6b5e4010c9e5adcd72c44a7a799359893cf20abd27ee3d2ba845d08132f998cd82a5827000171a0e40220cecdb58a95df93eae8e28bc14d1bc832a0528db8ab593fa996560add1d511430d82a5827000171a0e40220c24231c099a8d8dc5d829060a64e28b38e5aae09527ba54365b3f5ae7cc07d5b586102828268aa61490e93d026433a3502f9505c1c290f558ef9a318b85b10a3b1dd10d8f2dc083c67feb50556b0cb201d4e49162a10bc98c8403ca96ed95de5317441182f657c211e6652fc3d06e596728888df25ec6aa830ac01b4151b83f60102eb1a68816ed458610297d04e238290b1c8abdba49c3f4c3825d3783be60dd42de32f9e86876fbecda986e3a739def814dc3a2e153b4013367409cbf3dcf9b6fe8269ca69a7043b5c89d7c1c5594d591983277fa6e0a254ce74548e005cd103eda8b05a40b33ccbb16f00420064").unwrap();
    let bh_2 = from_slice_with_fallback::<CachingBlockHeader>(&bz).unwrap();

    // Store bh_1 in the database
    db.put(&bh_1).expect("Failed to add bh_1 to history");

    // Check if we have a block from the same miner at the same epoch
    let epoch_key = format!("{}/{}", bh_1.miner_address, bh_1.epoch);
    let existing_block_cid = db
        .get(
            crate::slasher::db::SlasherDbColumns::ByEpoch as u8,
            epoch_key.as_bytes(),
        )
        .expect("Failed to get existing block");

    assert!(existing_block_cid.is_some());
    assert_ne!(bh_1.cid(), bh_2.cid());
    assert_eq!(bh_1.epoch, bh_2.epoch);
    assert_eq!(bh_1.miner_address, bh_2.miner_address);
}

#[test]
fn test_filter_time_offset_mining() {
    use crate::slasher::db::SlasherDb;

    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("slasher_test_time_offset"));
    let mut db = SlasherDb::new(std::env::temp_dir().join("slasher_test_time_offset"))
        .expect("Failed to create slasher db");

    let miner = Address::new_id(1000);
    let epoch1 = 100;
    let epoch2 = 101;
    let parents = TipsetKey::from(nunny::vec![Cid::default()]);

    // Process first block with specific parents at epoch1
    let header1 = create_test_header(miner, epoch1, parents.clone(), Some(1000));
    db.put(&header1)
        .expect("Failed to add first block to history");

    // Process second block with same parents but different epoch - should detect time-offset mining
    let header2 = create_test_header(miner, epoch2, parents.clone(), Some(2000));

    // Check if we have a block from the same miner with the same parents
    let parent_key = format!("{}/{}", miner, parents);
    let existing_block_cid = db
        .get(
            crate::slasher::db::SlasherDbColumns::ByParents as u8,
            parent_key.as_bytes(),
        )
        .expect("Failed to get existing block");

    assert!(existing_block_cid.is_some());

    // Verify the blocks have same parents but different epochs (time offset mining)
    assert_eq!(header1.parents, header2.parents);
    assert_ne!(header1.epoch, header2.epoch);
    assert_eq!(header1.miner_address, header2.miner_address);
}

#[test]
fn test_filter_parent_grinding() {
    use crate::slasher::db::SlasherDb;
    use crate::utils::encoding::from_slice_with_fallback;

    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("slasher_test_parent_grinding"));
    let mut db = SlasherDb::new(std::env::temp_dir().join("slasher_test_parent_grinding"))
        .expect("Failed to create slasher db");

    let bz = hex::decode("904400ffdd08815860b04c19255dcbd71a139a05a415a75bf4af0ba577494792109615482019ea33541093767090ec66bb04b7356ca57ba22a0b840b8f4625a280a464e5457e913632ffddd3fdd57c149263d2ff6f8b87e4ecd7c878caf69b0e8c13c184e6ea38fee982045860911b306d6d32366f1280a40293133721adab7d68f1c56f3fa38b4da4a572445a2d9d36382028ccfda03e147c9e8dcef70df83279d935af5b00854c12e0054de9d2c6edcaefbd7a415020a9c665b3a2216a60b3132b6926460d3c4088db450eb881821a0139bbac5830b085236103a8eba6babafcaed69dc1fb8d39c87e8cdddfbecd0e6f18a2d398858ac9e2995afa6eaf264c0c30f990954c81820358c0823a1a4972fa3f59976bc4f2887b927fd10d6b6f9694f9a5498d3e12cc2954af481925745e818a357abb92cbfe882bad8ac9fdf64855a7a121bf30bfb4acbfb10af91fa35144d3034ef4717db584bb9ef022b81e15da6dde235335850ab6198403ee5f5e37250e9b9d429e38798ad16a244f2d040a01e97e19e2d8b6505af816b6947a528a0ac495a75faf5e06d99a53a62b90ea6d21c4ce889e77795ee5b5371b5717dcbb2168d628d20ee19253ecabea6e257bcc6943f4ab547275e7a4582781d82a5827000171a0e4022019ac618ee9295d815999a8a09ce9bdaf506191be596f824c2e0f1733eeaf936446000c98c26cff1a002c54e3d82a5827000171a0e4022046510e5272d2a3b81420859cea5923b1e5229584b6228052cc22b8418128bd9dd82a5827000171a0e402208b4b85acc72b4e4098d15585ab8309b8cbcad8d6e614d6b8eb455bcb7b7e94cfd82a5827000171a0e4022011edfe20d79962864f3e7dd5c03228dd1e52ee6818b060ec9497a5a3ab0d9665586102977b0e19fbf789f0ea9a08b33d5f6fe5bc6b541e4df2e7bb52d816f8f06cccd980227b1a450b37621066a97002436b6e04cb1d070412ec1cbd74e8b97ef7417f42c81929348c6fd301a8d14d65cf1f00d0ef7d042f7f2b6c780b894e6d67c1911a68935446586102a7448bafb337e7974d1ef830286eb19c7122ae0e1ceb50414cab14931641a1fa4a34ac81743361092250c6b02d135bbc09f8ca06d502401b630927630ef4a04f6156f52bf233ab431ff9255243d8d9a33ddb6f374bf3f23817fe9b48663b790800420064").unwrap();
    let bh_1 = from_slice_with_fallback::<CachingBlockHeader>(&bz).unwrap();

    let bz = hex::decode("904400ffdd088158609760b7d7e5c7ebe82182014778922df6d9649025f3bc4eeec58da24e049ec4b03cc2809a7bc82c67e993796cbe0bef7904827183cf3b56a4e400bca5af4e1a865d1f19984e91f758587b4933458ccc18bec7cbf1f4468bba94d195b0c608746a82045860a90908091c11146aafeddcdc3383627f90fe39c6b51921a22d8a67689b36956bb34c51c0ceffc1c1ab2c30a799e96e5f10841408199c8af7872a836c13234cb13822146b72abf16c644c0de5c7ebf4f75e7e8adf54722137e47ca21d6d0804f281821a0139bbb658309074e04e44a75a28d67ed61e5b53988e14d9d1103a5f33176409eef03f8806485714c865a27d065dae1a870c317e029781820358c0965052a879f191246b1dbdaee02c9d6c1a9c3b50dd03eb27d10421ff9e896d3faeb61e239e484bfb7529fadd531a1178b3ca87c1c0afceb8735670f365b72e06d4a4acdc1a8132202c0ca2fde64722e1da65cab97a8f97cd2ab66055cb09291913011e7827a180bf8c2c9c0a8a8736a5565d0ff681b3ea577643bc7c1f4edb1978d3c1cca6f953f85057e071827474e0b540c57c206c1914c9e6bc97a22418f3ed8946b0d82d7d4aa7c567ecf1cec8f3fd5ec94474b4e37225488909aea3a35382d82a5827000171a0e40220d6e7eff6b3fdeb53bae47c521a3ede40a8c50bc9e5118d275d72e71275a6a708d82a5827000171a0e40220577207a8ec6039e77c28df82c922137ba5d61de047dec5c0b629e1d937974f8846000c98c2adff1a002c54e4d82a5827000171a0e4022091d807ce5c9458384b9ef33634da5d4c1709d68a4ca134382ba6b062cf545894d82a5827000171a0e402200ef4990e62c56026e1cddb23b8b7e4abd1e359ed23cf8de4943945dc9ba71bffd82a5827000171a0e4022049c5d1c0fb44e016f7a3d86b66e0592b6b337538b89758240c356cc8ff1d5671586102c000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001a68935464586102b6a7086307f6a386ec683f0c965bb52681e61406746da05091b3204b65e68881e506f8fc97aee42b386099955cdc6a9b0649549f87f3d3d63bdfb2c4620e597156a2e9c07f28080c9dde198aa9ae31d536d9d4f2d79f0b1ec2413d84a7e2b89200420064").unwrap();
    let bh_2 = from_slice_with_fallback::<CachingBlockHeader>(&bz).unwrap();

    let extra_bytes = hex::decode("904300c81f8158608d7bafd98705e7700e3971bb74df794f0f7cc097444f4f963c7128712512caf725f46d529d2538438ee3a598a2f424f4017e31844d9f3da319c2be9ef5e1c6bac88da3fa86f1896f5ee7de34f9351bdcdf976dc88ccf4e35a8a293187140d8f482025860b452e3cb711cb701dc113b270e8b3861273753501f2db098ce4e7bdb85fbaa90f1159095981767371df95d8017dfdd1f015ffc0e3fb6969be1137feb7bfaff77c057d342436becfe9b278c76449bdd90811233ebbf56649aea8efaf6a2015e6d81821a0139bbac5830b085236103a8eba6babafcaed69dc1fb8d39c87e8cdddfbecd0e6f18a2d398858ac9e2995afa6eaf264c0c30f990954c81820358c08e4042d58de8a4b577324d89a62cd2c176ff46e2d63a9498e64c76095e617c6314964014675fe1d65c27096b7d92d122afc4f9d37de4d54402e454e27a38c0dace2db49866c95643f3dc3f087f9e0289dccab082a9a3b0f1b1e0ecd41f50c44e0df18344052d54c2790ba950b0017751bbd67a6a2aeace3401820a951af8c73063d2b571d6f8126b79f803770f96180ab6eca113d85160640e741d51f53ac19b319ac0587cf91d436b2c6385fc19ca65773de0a346d341610b26040fac11aaee81d82a5827000171a0e4022019ac618ee9295d815999a8a09ce9bdaf506191be596f824c2e0f1733eeaf936446000c98c26cff1a002c54e3d82a5827000171a0e4022046510e5272d2a3b81420859cea5923b1e5229584b6228052cc22b8418128bd9dd82a5827000171a0e402208b4b85acc72b4e4098d15585ab8309b8cbcad8d6e614d6b8eb455bcb7b7e94cfd82a5827000171a0e402200424e15c474771ccb2bb22fdb81e99b4c9f916f12e46dda7e3ae95f5a131e480586102c000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001a689354465861029577f33431fc443136630d65c6321b3b1f66b638f8816fc0915236cb622f60a2f12f9aaf8a1a11c99815d6dbe39273be1385d8dbdbec3777bdd6343a0a43b97aff74583f56bb3cdf46b553cd12c4c986d55aab6f7c883129735eb5884754d3f401420064").unwrap();
    let bh_3 = from_slice_with_fallback::<CachingBlockHeader>(&extra_bytes).unwrap();

    // Store bh_1 (miner's block) in the database
    db.put(&bh_1).expect("Failed to add bh_1 to history");

    // Store bh_3 (sibling block) in the database
    db.put(&bh_3).expect("Failed to add bh_3 to history");

    // Check if we have the miner's block stored
    let miner_epoch_key = format!("{}/{}", bh_1.miner_address, bh_1.epoch);
    let existing_block_cid = db
        .get(
            crate::slasher::db::SlasherDbColumns::ByEpoch as u8,
            miner_epoch_key.as_bytes(),
        )
        .expect("Failed to get existing block");

    assert!(existing_block_cid.is_some());
    assert_eq!(bh_1.epoch, bh_3.epoch);
    assert_eq!(bh_1.parents, bh_3.parents);
    assert!(bh_2.parents.contains(*bh_3.cid()));
    assert!(!bh_2.parents.contains(*bh_1.cid()));
}

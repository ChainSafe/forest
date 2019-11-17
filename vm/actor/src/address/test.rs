#![cfg(all(test))]

use crate::{checksum, validate_checksum, Address, Network};

#[test]
fn bytes() {
    let data = vec![0, 3, 2, 2, 4, 3, 2, 1, 3, 2, 1, 1, 3, 5, 7, 2, 4, 2, 1, 4];
    let new_addr = Address::new_secp256k1(data.clone()).unwrap();
    let encoded_bz = new_addr.to_bytes();

    // Assert decoded address equals the original address and a new one with the same data
    let decoded_addr = Address::from_bytes(encoded_bz).unwrap();
    assert!(decoded_addr == new_addr);
    assert!(decoded_addr == Address::new_secp256k1(data.clone()).unwrap());

    // Assert different types don't match
    assert!(decoded_addr != Address::new_actor(data.clone()).unwrap());
}

#[test]
fn generate_validate_checksum() {
    let data: Vec<u8> = vec![0, 2, 3, 4, 5, 1, 2];
    let other_data: Vec<u8> = vec![1, 4, 3, 6, 7, 1, 2];

    let cksm = checksum(data.clone());
    assert_eq!(cksm.len(), 4);

    assert_eq!(validate_checksum(data.clone(), cksm.clone()), true);
    assert_eq!(validate_checksum(other_data.clone(), cksm.clone()), false);
}

struct AddressTestVec {
    input: Vec<u8>,
    expected: &'static str,
}

#[test]
fn test_secp256k1_address() {
    let test_vectors = vec![
        AddressTestVec {
            input: vec![
                4, 148, 2, 250, 195, 126, 100, 50, 164, 22, 163, 160, 202, 84, 38, 181, 24, 90,
                179, 178, 79, 97, 52, 239, 162, 92, 228, 135, 200, 45, 46, 78, 19, 191, 69, 37, 17,
                224, 210, 36, 84, 33, 248, 97, 59, 193, 13, 114, 250, 33, 102, 102, 169, 108, 59,
                193, 57, 32, 211, 255, 35, 63, 208, 188, 5,
            ],
            expected: "t15ihq5ibzwki2b4ep2f46avlkrqzhpqgtga7pdrq",
        },
        AddressTestVec {
            input: vec![
                4, 118, 135, 185, 16, 55, 155, 242, 140, 190, 58, 234, 103, 75, 18, 0, 12, 107,
                125, 186, 70, 255, 192, 95, 108, 148, 254, 42, 34, 187, 204, 38, 2, 255, 127, 92,
                118, 242, 28, 165, 93, 54, 149, 145, 82, 176, 225, 232, 135, 145, 124, 57, 53, 118,
                238, 240, 147, 246, 30, 189, 58, 208, 111, 127, 218,
            ],
            expected: "t12fiakbhe2gwd5cnmrenekasyn6v5tnaxaqizq6a",
        },
        AddressTestVec {
            input: vec![
                4, 222, 253, 208, 16, 1, 239, 184, 110, 1, 222, 213, 206, 52, 248, 71, 167, 58, 20,
                129, 158, 230, 65, 188, 182, 11, 185, 41, 147, 89, 111, 5, 220, 45, 96, 95, 41,
                133, 248, 209, 37, 129, 45, 172, 65, 99, 163, 150, 52, 155, 35, 193, 28, 194, 255,
                53, 157, 229, 75, 226, 135, 234, 98, 49, 155,
            ],
            expected: "t1wbxhu3ypkuo6eyp6hjx6davuelxaxrvwb2kuwva",
        },
        AddressTestVec {
            input: vec![
                4, 3, 237, 18, 200, 20, 182, 177, 13, 46, 224, 157, 149, 180, 104, 141, 178, 209,
                128, 208, 169, 163, 122, 107, 106, 125, 182, 61, 41, 129, 30, 233, 115, 4, 121,
                216, 239, 145, 57, 233, 18, 73, 202, 189, 57, 50, 145, 207, 229, 210, 119, 186,
                118, 222, 69, 227, 224, 133, 163, 118, 129, 191, 54, 69, 210,
            ],
            expected: "t1xtwapqc6nh4si2hcwpr3656iotzmlwumogqbuaa",
        },
        AddressTestVec {
            input: vec![
                4, 247, 150, 129, 154, 142, 39, 22, 49, 175, 124, 24, 151, 151, 181, 69, 214, 2,
                37, 147, 97, 71, 230, 1, 14, 101, 98, 179, 206, 158, 254, 139, 16, 20, 65, 97, 169,
                30, 208, 180, 236, 137, 8, 0, 37, 63, 166, 252, 32, 172, 144, 251, 241, 251, 242,
                113, 48, 164, 236, 195, 228, 3, 183, 5, 118,
            ],
            expected: "t1xcbgdhkgkwht3hrrnui3jdopeejsoatkzmoltqy",
        },
        AddressTestVec {
            input: vec![
                4, 66, 131, 43, 248, 124, 206, 158, 163, 69, 185, 3, 80, 222, 125, 52, 149, 133,
                156, 164, 73, 5, 156, 94, 136, 221, 231, 66, 133, 223, 251, 158, 192, 30, 186, 188,
                95, 200, 98, 104, 207, 234, 235, 167, 174, 5, 191, 184, 214, 142, 183, 90, 82, 104,
                120, 44, 248, 111, 200, 112, 43, 239, 138, 31, 224,
            ],
            expected: "t17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy",
        },
    ];

    for t in test_vectors.iter() {
        let res = Address::new_secp256k1(t.input.clone()).unwrap();
        assert_eq!(
            t.expected.to_owned(),
            res.to_string(Some(Network::Testnet)).unwrap()
        );

        // TODO: finish testing with decoding from string
    }
}

// TODO: Add other protocol tests

#[cfg(test)]
use crate::{checksum, validate_checksum, Address};

#[test]
fn protocol_version() {
    // let new_addr = Address::new(Protocol::BLS, Vec::new()).unwrap();
    // assert!(new_addr.protocol() == Protocol::BLS);
    // assert!(new_addr.protocol() != Protocol::Undefined);
}

#[test]
fn payload() {
    // let data = vec![0, 1, 2];
    // let new_addr = Address::new(Protocol::Undefined, data.clone()).unwrap();
    // assert_eq!(new_addr.payload(), data);
}

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

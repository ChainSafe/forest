use ferret_bigint::{BaseUBigInt, UBigInt};

#[test]
fn test_lower_hex() {
    let a = UBigInt::from(BaseUBigInt::parse_bytes(b"A", 16).unwrap());
    let hello = UBigInt::from(
        BaseUBigInt::parse_bytes("22405534230753963835153736737".as_bytes(), 10).unwrap(),
    );

    assert_eq!(format!("{:x}", a), "a");
    assert_eq!(format!("{:x}", hello), "48656c6c6f20776f726c6421");
    assert_eq!(format!("{:♥>+#8x}", a), "♥♥♥♥+0xa");
}

#[test]
fn test_upper_hex() {
    let a = UBigInt::from(BaseUBigInt::parse_bytes(b"A", 16).unwrap());
    let hello = UBigInt::from(
        BaseUBigInt::parse_bytes("22405534230753963835153736737".as_bytes(), 10).unwrap(),
    );

    assert_eq!(format!("{:X}", a), "A");
    assert_eq!(format!("{:X}", hello), "48656C6C6F20776F726C6421");
    assert_eq!(format!("{:♥>+#8X}", a), "♥♥♥♥+0xA");
}

#[test]
fn test_binary() {
    let a = UBigInt::from(BaseUBigInt::parse_bytes(b"A", 16).unwrap());
    let hello = UBigInt::from(BaseUBigInt::parse_bytes("224055342307539".as_bytes(), 10).unwrap());

    assert_eq!(format!("{:b}", a), "1010");
    assert_eq!(
        format!("{:b}", hello),
        "110010111100011011110011000101101001100011010011"
    );
    assert_eq!(format!("{:♥>+#8b}", a), "♥+0b1010");
}

#[test]
fn test_octal() {
    let a = UBigInt::from(BaseUBigInt::parse_bytes(b"A", 16).unwrap());
    let hello = UBigInt::from(
        BaseUBigInt::parse_bytes("22405534230753963835153736737".as_bytes(), 10).unwrap(),
    );

    assert_eq!(format!("{:o}", a), "12");
    assert_eq!(format!("{:o}", hello), "22062554330674403566756233062041");
    assert_eq!(format!("{:♥>+#8o}", a), "♥♥♥+0o12");
}

#[test]
fn test_display() {
    let a = UBigInt::from(BaseUBigInt::parse_bytes(b"A", 16).unwrap());
    let hello = UBigInt::from(
        BaseUBigInt::parse_bytes("22405534230753963835153736737".as_bytes(), 10).unwrap(),
    );

    assert_eq!(format!("{}", a), "10");
    assert_eq!(format!("{}", hello), "22405534230753963835153736737");
    assert_eq!(format!("{:♥>+#8}", a), "♥♥♥♥♥+10");
}

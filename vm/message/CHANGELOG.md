# 0.7.1 [2021-03-25]

- Changed `blst` to default, and added `pairing` flag to use pairings instead of blst.
# 0.7.0 [2021-03-25]

- Upgrade to forest_crypto 0.5
# 0.5.0 [2020-10-02]

- Fixes message verification for `new_from_parts` function.
  See [issue 1622](https://github.com/ChainSafe/forest/issues/725).

- Adds `verify` function which verifies the `SignedMessage` using it's `Signature`, from `Address` from `UnsignedMessage` and signing bytes from the message's `Cid` bytes.

- Adds `to_signing_bytes` which is a helper function that generates the signing bytes from an `UnsignedMessage`

- Updates `Signer` trait to correctly take in the message bytes as a slice.

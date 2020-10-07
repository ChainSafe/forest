# 0.5.0 [2020-10-02]

- Fixes message verification for `new_from_parts` function.
  See [issue 1622](https://github.com/ChainSafe/forest/issues/725).

- Adds `verify` function which verifies the `SignedMessage` using it's `Signature`, from `Address` from `UnsignedMessage` and signing bytes from the message's `Cid` bytes.

- Adds `to_signing_bytes` which is a helper function that generates the signing bytes from an `UnsignedMessage`

- Updates `Signer` trait to correctly take in the message bytes as a slice.

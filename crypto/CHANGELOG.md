# 0.5.0 [UNRELEASED]

- Removed `from_byte` for `DomainSeparationTag`. If this is needed, can use the `FromPrimitive` trait.
- Removes `Default` for `Signature`. This was an old need for when block signatures were not optional in `BlockHeader`s.
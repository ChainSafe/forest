# 0.5.0 [UNRELEASED]

- Removed `from_byte` for `DomainSeparationTag`. Repr is i64 and conversion should just be through casting.
- Removes `Default` for `Signature`. This was an old need for when block signatures were not optional in `BlockHeader`s.
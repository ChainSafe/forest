# 0.5.2

- `pairing` feature doesn't use Rayon anymore so it can compile to wasm
# 0.5.1

- Changed `blst` to a default feature, and added `pairing` flag to use pairings instead.
# 0.5.0 

- Removed `from_byte` for `DomainSeparationTag`. If this is needed, can use the `FromPrimitive` trait.
- Removes `Default` for `Signature`. This was an old need for when block signatures were not optional in `BlockHeader`s.
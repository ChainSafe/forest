# 2.0.0 [UNRELEASED]

- `set_if_absent` function added. This inserts a value only if the key does not already exist in the Hamt.
- `set` now doesn't require flushes when the value set is equal to the one that already exists. This is needed for go interop and removes the need for an extra flush or checking if exists in a separate operation.
- `set` now returns the value that existed for that Key, or `None` if there was no value at that index previously.
- Updates the serialization format of the `Pointer` type to be a kinded union rather than keyed Cbor bytes. This is also needed for `v3` hamt go interop
- Actors v3 compatible version

# 1.0.0 [2021-02-03]

- `v2` feature removed
- Actors v2 compatible version.

# 0.1.1 [2020-12-11]

- v0 Actors compatible Hamt. This release is the same as v2, except that the bug fix for delete reordering is under feature `v2`
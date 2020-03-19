# Sector Builder

The Sector Builder provides a database of sectors, implemented in Rust as a crate. It will eventually manage the configuration and implementation of access to sectors (sealed and unsealed) and the disk cache required to perform the operations of `filecoin-proofs` and `storage-proofs`.

These methods will include at least:
- Incremental writing of pieces to sectors
- Preprocessing raw sectors to add bit-level padding
- [In reality, the two items above are merged: we preprocess as we write.]
- Managing caching and generation of multiple representations of sectors as required
- Merkle-tree caching/re-generation

## A word on 'padding'

There are two types of padding performed on raw data before it can be replicated. For the sake of simplicity, we will refer to these as byte-padding and bit-padding. Byte-padding refers to (zero or more) bytes of zeroes added to the end of raw data in order to bring its total size up to the expected sector size.

Bit-padding, also called 'preprocessing', refers to the two zero *bits* added after every 254 bits of raw data. Padding is necessary (in the current implementation) so that successive logical field elements (Fr) will be byte-aligned. Because the modulus of the field is a 255-bit prime, there are some 255-bit numbers which are NOT elements of our field. Therefore, in order to ensure we never overflow the field, we can take at most 254 bits of real data for every 256 bits (32 bytes) of byte-aligned storage.

## License

MIT or Apache 2.0

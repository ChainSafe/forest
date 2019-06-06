# Sector Builder

The Sector Builder provides a database of sectors, implemented in Rust as a crate and exporting a C API for consumption by (for example) `go-filecoin`. It will eventually manage the configuration and implementation of access to sectors (sealed and unsealed) and the disk cache required to perform the operations of `filecoin-proofs` and `storage-proofs`.

These methods will include at least:
- Incremental writing of pieces to sectors
- Preprocessing raw sectors to add bit-level padding
- [In reality, the two items above are merged: we preprocess as we write.]
- Managing caching and generation of multiple representations of sectors as required
- Merkle-tree caching/re-generation

The Sector Base's responsibilities may extend to:
- Random access to sectors and their nodes across heterogeneous storage types and formats
- Sector metadata management
- Sector-packing
- Sector-building
- Construction of aggregate merkle trees (merging multiple sectors to generate a single root)

Or even possibly:
- Random access to sectors indexed by field element (further reducing padding)

## A word on 'padding'

There are two types of padding performed on raw data before it can be replicated. For the sake of simplicity, we will refer to these as byte-padding and bit-padding. Byte-padding refers to (zero or more) bytes of zeroes added to the end of raw data in order to bring its total size up to the expected sector size.

Bit-padding, also called 'preprocessing', refers to the two zero *bits* added after every 254 bits of raw data. Padding is necessary (in the current implementation) so that successive logical field elements (Fr) will be byte-aligned. Because the modulus of the field is a 255-bit prime, there are some 255-bit numbers which are NOT elements of our field. Therefore, in order to ensure we never overflow the field, we can take at most 254 bits of real data for every 256 bits (32 bytes) of byte-aligned storage.

## Design Notes

### Motivation for Sector Base as a Rust module. Why not part of `go-filecoin`?
   We need to expose some control over disk/file storage at a level that is difficult to do from `go-filecoin` and across the FFI boundary.
-   Example: Miners may want to treat disks as raw block devices without a filesystem.

  For example: Go code may pass a Go pointer to C, provided the Go memory to which it points does not contain any Go pointers. Our earlier attempts to write this disk/storage manager in Go involved passing a pointer to a function defined in Go to Rust, which, when applied to some key, returned a value from a Go map (this simulated the FPS acquiring file paths from the disk/file manager). In addition to this pointer, we passed the key. This panicked at runtime with a message "cgo argument has Go pointer to Go pointer," which makes sense given the [cgo documentation](https://golang.org/cmd/cgo/). This isn't a problem if we let Rust manage the disk/storage manager state.

   Moreover, this specific problem is emblematic of the friction imposed by the FFI boundary. In fact, communication between the Sector Base and the proving components of FPS (`filecoin-proofs` and `storage-proofs`) will need to be carefully coordinated for both efficiency and correctness. The issue is less that Go is problematic than that the FFI boundary will interfere with optimal function. That said, Rust’s type system will be quite helpful for the kind of performant genericity required. Although this naturally lives on the Rust side of that API boundary, the concerns controlling these functions include those which are only peripherally related to the proofs themselves.

 Configuration and tuning of storage concerns will depend on considerations derived from interacting with the Filecoin protocol and blockchain in ways which have potentially nothing to do with the storage proofs. This logic should not be contained in the proof modules. These two considerations delimit what the Sector Base is *not* (part of `go-filecoin` itself, or part of `storage-proofs` which must consume it). This also explains why `sector-base` currently exists as an independent crate packaged under the umbrella of the Filecoin Proving Subsystem (FPS) with source in the `rust-fil-proofs` repository.

## API Reference
[**Sector Base API**](https://github.com/filecoin-project/rust-fil-proofs/blob/master/sector-base/src/api/mod.rs). The Rust source code serves as the source of truth defining the **Sector Base** API and will be the eventual source of generated documentation.

[**DiskBackedSectorStore API**](https://github.com/filecoin-project/rust-fil-proofs/blob/master/sector-base/src/api/disk_backed_storage.rs)

## License

MIT or Apache 2.0

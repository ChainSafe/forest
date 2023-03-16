# Testing for Mainnet Compatibility

Forest development can be like hitting a moving target and sometimes Forest
falls behind the network. This document should serve as a way to easily identify
if Forest can sync all the way up to the network head using a simple
step-by-step process.

## Prerequisites

Some command-line tools and software is required to follow this guide.

- A fresh copy of the Forest repository that has been built
- Lotus installed
- curl (to download snapshots)
- sha256sum (optional, used to verify snapshot integrity)

## Grab a snapshot and run Forest

Refer to the mdbook documentation on how to download a snapshot and run forest

Warning: FileCoin snapshots as of this writing are over 75GB. Verify you have
enough space on your system to accommodate these large files.

- Use `make mdbook` in Forest's root directory
- Open `http://localhost:3000`
- Navigate to `2. Basic Usage` in the menu on the right
- Scroll down to `Forest Import Snapshot Mode`

## Let Forest sync

This step may take a while. We want Forest to get as far along in the syncing
process as it can get. If it syncs up all the way to the network head, CONGRATS!
Forest is up to date and on mainnet. Otherwise, Forest is not on mainnet.

If Forest starts to error and can't get past a block while syncing. Make note of
which block it is. We can use that block to help debug any potential state
mismatches.

## Is Forest on the latest network version?

Something easy to check is if Forest is on the latest Filecoin network version.
A repository exists where we can see all of the released network versions
[here](https://github.com/filecoin-project/tpm/tree/master/Network%20Upgrades).
Navigate the codebase to see mention of the latest network upgrade. If a
snapshot fails to sync at a certain epoch, it's entirely possible that the
snapshot was behind an epoch when a version upgrade started. Grab a new snapshot
by referring to the mdbook documentation.

## Debugging State Mismatches

Statediffs can only be printed if we import a snapshot containing the stateroot
data from Lotus. This means there will not be a pretty statediff if Forest is
already synced to the network when the stateroot mismatch happens. By default,
snapshots only contain stateroot data for the previous 2000 epochs. So, if you
have a statediff at epoch X, download a snapshot for epoch X+100 and tell Forest
to re-validate the snapshot from epoch X.

For more detailed instructions, follow
[this document](https://www.notion.so/chainsafe/Interop-debugging-6adabf9222d7449bbfeaacb1ec997cf8)

## FVM Traces

Within FVM, we can enable tracing to produce execution traces. Given an
offending epoch, we can produce them both for Forest and for Lotus to find
mismatches.

To confirm: the execution traces format is not uniform across implementations,
so it takes a certain amount of elbow grease to find the differences. Lotus is
capable of spitting this out in JSON for nice UX

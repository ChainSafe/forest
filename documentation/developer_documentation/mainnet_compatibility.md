
# Testing for Mainnet Compatibility

Forest development can be like hitting a moving target and sometimes Forest falls behind the
network. This document should serve as a way to easily identify if Forest can sync all the way
up to the network head using a simple step-by-step process.

## Prerequisites

Some command-line tools and software is required to follow this guide. 

 - A fresh copy of the Forest repository that has been built
 - Lotus installed
 - curl (to download snapshots)
 - sha256sum (optional, used to verify snapshot integrity)


## Grab a snapshot and run Forest

Refer to the mdbook documentation on how to download a snapshot and run forest

Warning: FileCoin snapshots as of this writing are over 75GB. Verify you have enough
space on your system to accomodate these large files.

 - Use `make mdbook` in Forest's root directory
 - Open `http://localhost:3000`
 - Navigate to `2. Basic Usage` in the menu on the right
 - Scroll down to `Forest Import Snapshot Mode`


## Let Forest sync

This step may take a while. We want Forest to get as far along in the syncing process
as it can get. If it syncs up all the way to the network head, CONGRATS! Forest is up to date
and on mainnet. Otherwise, Forest is not on mainnet.


## Debugging State Mismatches

If Forest starts to error and can't get past a block while syncing. Make note of which block it is.
We can use that block to help debug any potential state mismatches.
